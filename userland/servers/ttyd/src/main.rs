// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ttyd`: session/foreground-group state server.
//!
//! `OPEN ttyd:session` allocates a session. The 12-byte CALL subset is:
//! `SET_FG(arg0)`, `GET_FG`, `HANGUP`, `STATE` (arg0 fg, arg1 sid with bit 31 = hung).

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};

    use raven_abi::errno::Errno;
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_rt::entry;
    use raven_scheme::{CallReply, Ctx, Handler, HandlerResult, Opened, Server};
    use raven_ttyd::{Peers, Sessions};

    entry!(main);

    const VERSION: u16 = 1;
    const CALL_LEN: usize = 12;
    const OP_SET_FG: u16 = 0x20;
    const OP_GET_FG: u16 = 0x21;
    const OP_HANGUP: u16 = 0x22;
    const OP_STATE: u16 = 0x23;
    const OP_SET_SID: u16 = 0x24;
    const OP_GET_SID: u16 = 0x25;
    const OP_SESSION_ID: u16 = 0x26;
    const OP_GET_WRITE: u16 = 0x27;
    const OP_SET_WRITE: u16 = 0x28;
    const OP_PEER_LOOKUP: u16 = 0x30;
    const HUNG_BIT: u32 = 0x8000_0000;
    const PEER_BIT: u64 = 1 << 31;
    const LOOKUP_BIT: u64 = 1 << 30;
    const MAX_USER: usize = 64;
    const MAX_TTY: usize = 96;

    #[derive(Clone, Debug)]
    struct Lookup {
        user: String,
        terminal: Option<String>,
    }

    struct HandlerImpl {
        sessions: Sessions,
        refs: BTreeMap<u64, u32>,
        peers: Peers,
        peer_refs: BTreeMap<u64, u32>,
        lookups: BTreeMap<u64, Lookup>,
        next_lookup: u64,
    }

    impl HandlerImpl {
        fn new() -> Self {
            Self {
                sessions: Sessions::new(),
                refs: BTreeMap::new(),
                peers: Peers::new(),
                peer_refs: BTreeMap::new(),
                lookups: BTreeMap::new(),
                next_lookup: 1,
            }
        }

        fn session(&self, file: u64) -> Result<u32, Errno> {
            if file & (PEER_BIT | LOOKUP_BIT) != 0 {
                return Err(Errno::EBADF);
            }
            if self.refs.contains_key(&file) {
                u32::try_from(file).map_err(|_| Errno::EBADF)
            } else {
                Err(Errno::EBADF)
            }
        }

        fn peer(&self, file: u64) -> Result<u64, Errno> {
            let id = file & !PEER_BIT;
            if self.peers.get(id).is_some() {
                Ok(id)
            } else {
                Err(Errno::EBADF)
            }
        }

        fn lookup(&self, file: u64) -> Result<&Lookup, Errno> {
            self.lookups.get(&file).ok_or(Errno::EBADF)
        }

        fn new_lookup(&mut self, user: &str, terminal: Option<String>) -> u64 {
            let id = self.next_lookup;
            self.next_lookup = self.next_lookup.wrapping_add(1).max(1);
            let file = LOOKUP_BIT | id;
            self.lookups.insert(
                file,
                Lookup {
                    user: user.to_string(),
                    terminal,
                },
            );
            file
        }
    }

    impl Handler for HandlerImpl {
        fn open(
            &mut self,
            path: &str,
            flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let path = path.strip_prefix('/').unwrap_or(path);
            if flags & 4 != 0 {
                return Err(Errno::EINVAL.into());
            }
            let session = if path.is_empty() || path == "session" || path == "0" {
                let session = self.sessions.open();
                self.refs.insert(u64::from(session), 1);
                session
            } else if let Some(rest) = path.strip_prefix("session/") {
                let session = rest.parse::<u32>().map_err(|_| Errno::ENOENT)?;
                if self.sessions.get(session).is_none() {
                    return Err(Errno::ENOENT.into());
                }
                let count = self.refs.entry(u64::from(session)).or_insert(0);
                *count = count.saturating_add(1);
                session
            } else if let Some(rest) = path.strip_prefix("peer/") {
                let mut parts = rest.splitn(3, '/');
                let session = parts
                    .next()
                    .ok_or(Errno::ENOENT)?
                    .parse::<u32>()
                    .map_err(|_| Errno::ENOENT)?;
                let user = parts.next().ok_or(Errno::ENOENT)?;
                let tty = parts.next().ok_or(Errno::ENOENT)?;
                if !valid_name(user) || !valid_tty(tty) || self.sessions.get(session).is_none() {
                    return Err(Errno::EINVAL.into());
                }
                let writable = self.sessions.write_allowed(session);
                let id = self.peers.register(user, tty, session, writable);
                let file = PEER_BIT | id;
                self.peer_refs.insert(file, 1);
                return Ok(Opened::File(file));
            } else if let Some(rest) = path.strip_prefix("who/") {
                let (user, terminal) = match rest.split_once('/') {
                    Some((user, terminal)) => (user, Some(terminal.to_string())),
                    None => (rest, None),
                };
                if !valid_name(user) || terminal.as_deref().is_some_and(|tty| !valid_tty(tty)) {
                    return Err(Errno::EINVAL.into());
                }
                let file = self.new_lookup(user, terminal);
                return Ok(Opened::File(file));
            } else {
                return Err(Errno::ENOENT.into());
            };
            Ok(Opened::File(u64::from(session)))
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            if request.len() < CALL_LEN || response.len() < CALL_LEN {
                return Err(Errno::EINVAL.into());
            }
            let version = u16::from_le_bytes([request[0], request[1]]);
            if version != VERSION {
                return Err(Errno::EINVAL.into());
            }
            let op = u16::from_le_bytes([request[2], request[3]]);
            if file & PEER_BIT != 0 {
                let id = self.peer(file)?;
                if op != OP_PEER_LOOKUP {
                    return Err(Errno::ENOSYS.into());
                }
                let peer = self.peers.get(id).ok_or(Errno::EBADF)?;
                return encode_peer_reply(response, peer.writable, &peer.tty);
            }
            if file & LOOKUP_BIT != 0 {
                if op != OP_PEER_LOOKUP {
                    return Err(Errno::ENOSYS.into());
                }
                let lookup = self.lookup(file)?;
                let peer = self
                    .peers
                    .lookup(&lookup.user, lookup.terminal.as_deref())
                    .ok_or(Errno::ENOENT)?;
                return encode_peer_reply(response, peer.writable, &peer.tty);
            }
            let session = self.session(file)?;
            let arg0 = u32::from_le_bytes([request[4], request[5], request[6], request[7]]);
            let _arg1 = u32::from_le_bytes([request[8], request[9], request[10], request[11]]);
            let (reply_op, reply0, reply1) = match op {
                OP_SET_FG => {
                    if !self.sessions.set_fg(session, arg0 as i32) {
                        return Err(Errno::ESRCH.into());
                    }
                    (OP_SET_FG, self.sessions.fg(session) as u32, 0)
                }
                OP_GET_FG => (OP_GET_FG, self.sessions.fg(session) as u32, 0),
                OP_HANGUP => {
                    if !self.sessions.hangup(session) {
                        return Err(Errno::ESRCH.into());
                    }
                    (OP_HANGUP, 0, 0)
                }
                OP_STATE => {
                    let sid = self.sessions.sid(session) as u32;
                    let hung = if self.sessions.hung(session) {
                        HUNG_BIT
                    } else {
                        0
                    };
                    (OP_STATE, self.sessions.fg(session) as u32, sid | hung)
                }
                OP_SET_SID => {
                    if !self.sessions.set_sid(session, arg0 as i32) {
                        return Err(Errno::ESRCH.into());
                    }
                    (OP_SET_SID, arg0, 0)
                }
                OP_GET_SID => (OP_GET_SID, self.sessions.sid(session) as u32, 0),
                OP_SESSION_ID => (OP_SESSION_ID, u32::try_from(file).unwrap_or(0), 0),
                OP_GET_WRITE => (
                    OP_GET_WRITE,
                    u32::from(self.sessions.write_allowed(session)),
                    0,
                ),
                OP_SET_WRITE => {
                    let allowed = match arg0 {
                        0 => false,
                        1 => true,
                        _ => return Err(Errno::EINVAL.into()),
                    };
                    if !self.sessions.set_write_allowed(session, allowed) {
                        return Err(Errno::ESRCH.into());
                    }
                    self.peers.set_writable_for_session(session, allowed);
                    (OP_SET_WRITE, u32::from(allowed), 0)
                }
                _ => return Err(Errno::ENOSYS.into()),
            };
            response[..2].copy_from_slice(&VERSION.to_le_bytes());
            response[2..4].copy_from_slice(&reply_op.to_le_bytes());
            response[4..8].copy_from_slice(&reply0.to_le_bytes());
            response[8..12].copy_from_slice(&reply1.to_le_bytes());
            Ok(CALL_LEN)
        }

        fn call_with(
            &mut self,
            file: u64,
            request: &[u8],
            response: &mut [u8],
            ctx: &Ctx,
        ) -> HandlerResult<CallReply> {
            if request.len() >= 4 {
                let op = u16::from_le_bytes([request[2], request[3]]);
                if op == OP_SET_WRITE && ctx.credentials.euid != 0 {
                    return Err(Errno::EPERM.into());
                }
            }
            Ok(CallReply::new(self.call(file, request, response)?))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            if file & PEER_BIT != 0 {
                self.peer(file)?;
                return if buffer.is_empty() {
                    Ok(0)
                } else {
                    Err(Errno::EAGAIN.into())
                };
            }
            if file & LOOKUP_BIT != 0 {
                self.lookup(file)?;
                return if buffer.is_empty() {
                    Ok(0)
                } else {
                    Err(Errno::EAGAIN.into())
                };
            }
            self.session(file)?;
            if buffer.is_empty() {
                Ok(0)
            } else {
                // The data path belongs to ptyd; the session handle still passes the kernel's
                // controlling-terminal check and has no data of its own.
                Err(Errno::EAGAIN.into())
            }
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            if file & (PEER_BIT | LOOKUP_BIT) != 0 {
                return Err(Errno::EACCES.into());
            }
            self.session(file)?;
            Ok(buffer.len())
        }

        fn write_with(
            &mut self,
            file: u64,
            offset: u64,
            buffer: &[u8],
            ctx: &Ctx,
        ) -> HandlerResult<usize> {
            let session = self.session(file)?;
            if !self.sessions.write_allowed(session) && ctx.credentials.euid != 0 {
                return Err(Errno::EACCES.into());
            }
            self.write(file, offset, buffer)
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            if file & PEER_BIT != 0 {
                self.peer(file)?;
                return Ok(Stat {
                    ino: file,
                    mode: S_IFCHR | 0o600,
                    nlink: 1,
                    ..Stat::default()
                });
            }
            if file & LOOKUP_BIT != 0 {
                self.lookup(file)?;
                return Ok(Stat {
                    ino: file,
                    mode: S_IFCHR | 0o600,
                    nlink: 1,
                    ..Stat::default()
                });
            }
            let session = self.session(file)?;
            let mode = if self.sessions.write_allowed(session) {
                0o620
            } else {
                0o600
            };
            Ok(Stat {
                ino: file,
                mode: S_IFCHR | mode,
                nlink: 1,
                ..Stat::default()
            })
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            if file & PEER_BIT != 0 {
                self.peer(file)?;
                let count = self.peer_refs.entry(file).or_insert(0);
                *count = count.saturating_add(1);
                return Ok(file);
            }
            if file & LOOKUP_BIT != 0 {
                let lookup = self.lookup(file)?.clone();
                return Ok(self.new_lookup(&lookup.user, lookup.terminal));
            }
            self.session(file)?;
            let count = self.refs.entry(file).or_insert(0);
            *count = count.saturating_add(1);
            Ok(file)
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            if file & PEER_BIT != 0 {
                let Some(count) = self.peer_refs.get_mut(&file) else {
                    return Err(Errno::EBADF.into());
                };
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.peer_refs.remove(&file);
                    let _ = self.peers.unregister(file & !PEER_BIT);
                }
                return Ok(());
            }
            if file & LOOKUP_BIT != 0 {
                return self
                    .lookups
                    .remove(&file)
                    .map(|_| ())
                    .ok_or(Errno::EBADF.into());
            }
            let Some(count) = self.refs.get_mut(&file) else {
                return Err(Errno::EBADF.into());
            };
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.refs.remove(&file);
                if let Ok(session) = u32::try_from(file) {
                    let _ = self.sessions.close(session);
                    let _ = self.peers.unregister_session(session);
                }
            }
            Ok(())
        }
    }

    fn encode_peer_reply(out: &mut [u8], writable: bool, tty: &str) -> HandlerResult<usize> {
        let bytes = tty.as_bytes();
        let Some(total) = CALL_LEN.checked_add(bytes.len()) else {
            return Err(Errno::EINVAL.into());
        };
        if out.len() < total {
            return Err(Errno::EINVAL.into());
        }
        out[..2].copy_from_slice(&VERSION.to_le_bytes());
        out[2..4].copy_from_slice(&OP_PEER_LOOKUP.to_le_bytes());
        out[4..8].copy_from_slice(&u32::from(writable).to_le_bytes());
        out[8..12].copy_from_slice(&u32::try_from(bytes.len()).unwrap_or(0).to_le_bytes());
        out[12..total].copy_from_slice(bytes);
        Ok(total)
    }

    fn valid_name(text: &str) -> bool {
        !text.is_empty()
            && text.len() <= MAX_USER
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    }

    fn valid_tty(text: &str) -> bool {
        !text.is_empty()
            && text.len() <= MAX_TTY
            && text.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b'_' | b'-' | b':')
            })
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("ttyd") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("ttyd: {}", error.description());
                return 1;
            }
        };
        let mut handler = HandlerImpl::new();
        loop {
            if let Some(packet) = server.poll(None) {
                let _ = server.dispatch(packet, &mut handler);
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
