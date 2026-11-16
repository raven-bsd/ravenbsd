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

    use raven_abi::errno::Errno;
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};
    use raven_ttyd::Sessions;

    entry!(main);

    const VERSION: u16 = 1;
    const CALL_LEN: usize = 12;
    const OP_SET_FG: u16 = 0x20;
    const OP_GET_FG: u16 = 0x21;
    const OP_HANGUP: u16 = 0x22;
    const OP_STATE: u16 = 0x23;
    const HUNG_BIT: u32 = 0x8000_0000;

    struct HandlerImpl {
        sessions: Sessions,
        files: BTreeMap<u64, u32>,
        next: u64,
    }

    impl HandlerImpl {
        fn new() -> Self {
            Self {
                sessions: Sessions::new(),
                files: BTreeMap::new(),
                next: 1,
            }
        }

        fn session(&self, file: u64) -> Result<u32, Errno> {
            self.files.get(&file).copied().ok_or(Errno::EBADF)
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
            if !path.is_empty() && path != "session" && path != "0" {
                return Err(Errno::ENOENT.into());
            }
            if flags & 4 != 0 {
                return Err(Errno::EINVAL.into());
            }
            let id = self.next;
            self.next = self.next.wrapping_add(1);
            let session = self.sessions.open();
            self.files.insert(id, session);
            Ok(Opened::File(id))
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            let session = self.session(file)?;
            if request.len() < CALL_LEN || response.len() < CALL_LEN {
                return Err(Errno::EINVAL.into());
            }
            let version = u16::from_le_bytes([request[0], request[1]]);
            if version != VERSION {
                return Err(Errno::EINVAL.into());
            }
            let op = u16::from_le_bytes([request[2], request[3]]);
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
                _ => return Err(Errno::ENOSYS.into()),
            };
            response[..2].copy_from_slice(&VERSION.to_le_bytes());
            response[2..4].copy_from_slice(&reply_op.to_le_bytes());
            response[4..8].copy_from_slice(&reply0.to_le_bytes());
            response[8..12].copy_from_slice(&reply1.to_le_bytes());
            Ok(CALL_LEN)
        }

        fn fstat(&mut self, _file: u64) -> HandlerResult<Stat> {
            Ok(Stat {
                mode: S_IFCHR | 0o666,
                nlink: 1,
                ..Stat::default()
            })
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            let Some(session) = self.files.remove(&file) else {
                return Err(Errno::EBADF.into());
            };
            let _ = self.sessions.close(session);
            Ok(())
        }
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
