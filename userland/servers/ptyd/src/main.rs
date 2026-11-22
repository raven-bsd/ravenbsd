// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `pty:` master/slave server.
//!
//! `OPEN pty:master` and `OPEN pty:slave` select the two ends of one pair. Standard tty CALLs
//! (`SET_MODE`, `GET_MODE`, `FLUSH`) use `raven-proto-tty`; the server-local calls `SET_FG`,
//! `GET_FG` and `HANGUP` use the same 12-byte version/op/arg shape. Ctrl-C is delivered as SIGINT
//! to the foreground process group through `PROC_SIGNAL`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // The raw signal syscall below has a SAFETY comment.

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeMap;

    use raven_abi::errno::Errno;
    use raven_abi::types::clock;
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_proto_tty::{self, Request, Response, decode_request, encode_response};
    use raven_ptyd::{Pairs, SignalSink};
    use raven_rt::entry;
    use raven_rt::sys::{clock_get, nanosleep};
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};
    use raven_tty_core::{VMIN, VTIME};

    entry!(main);

    const PROC_SIGNAL: usize = 0x4A;
    const OP_SET_FG: u16 = 0x10;
    const OP_GET_FG: u16 = 0x11;
    const OP_HANGUP: u16 = 0x12;

    #[derive(Default)]
    struct TargetSignal;

    impl SignalSink for TargetSignal {
        fn signal(&mut self, pgrp: i32, signo: u32) -> Result<(), Errno> {
            // SAFETY: SIGNAL takes two scalar values; the kernel validates the negative group,
            // the signal number and the caller's uid permission.
            let raw = unsafe {
                raven_rt::sys::syscall2(PROC_SIGNAL, pgrp as isize as usize, signo as usize)
            };
            Errno::from_ret(raw).map(|_| ())
        }
    }

    /// Monotonic nanoseconds, or zero when the clock is unavailable.
    fn now_ns() -> u64 {
        clock_get(clock::MONOTONIC).map_or(0, |time| {
            (time.tv_sec as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(time.tv_nsec as u64)
        })
    }

    /// Slave read with `VMIN`/`VTIME` semantics.
    ///
    /// `VMIN`/`VTIME` are stored in the termios record; a slave read applies them here because
    /// the scheme server is the only place with a clock. The loop yields 1 ms between polls, so
    /// it does not busy-spin. `VTIME` is in deciseconds, as in termios. This mirrors the donor's
    /// read timer outcome; it is not the donor's wait queue implementation.
    fn read_slave_wait(
        pty: &mut raven_ptyd::Pty<TargetSignal>,
        buffer: &mut [u8],
    ) -> Result<usize, Errno> {
        let termios = pty.termios();
        let vmin = usize::from(termios.cc(VMIN));
        let vtime = u64::from(termios.cc(VTIME));
        match pty.read_slave(buffer) {
            Ok(count) => return Ok(count),
            Err(Errno::EAGAIN) => {}
            Err(error) => return Err(error),
        }
        if vmin == 0 && vtime == 0 {
            return Err(Errno::EAGAIN);
        }
        let start = now_ns();
        let limit = (vtime != 0).then_some(vtime.saturating_mul(100_000_000));
        loop {
            let _ = nanosleep(1_000_000);
            match pty.read_slave(buffer) {
                Ok(count) => return Ok(count),
                Err(Errno::EAGAIN) => {}
                Err(error) => return Err(error),
            }
            if let Some(limit) = limit
                && now_ns().saturating_sub(start) >= limit
            {
                return Ok(0);
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Role {
        Master,
        Slave,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct FileEntry {
        pair: u64,
        role: Role,
    }

    struct HandlerImpl {
        pairs: Pairs<TargetSignal>,
        files: BTreeMap<u64, FileEntry>,
        next: u64,
    }

    impl HandlerImpl {
        fn new() -> Self {
            Self {
                pairs: Pairs::new(),
                files: BTreeMap::new(),
                next: 1,
            }
        }

        fn file(&self, file: u64) -> Result<FileEntry, Errno> {
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
            let (pair, role) = match path {
                // `svcd` probes a resident scheme with the bare `pty:` path; pair 0 is the
                // historical named pair and is safe to create for the probe.
                "" | "master" => (0u64, Role::Master),
                // `posix_openpt` opens the clone device; the server allocates the unit, then
                // `fstat` gives the caller the pair id through the endpoint inode.
                "ptmx" => (self.pairs.alloc(), Role::Master),
                "slave" => (0u64, Role::Slave),
                _ => {
                    let (role, id) = if let Some(rest) = path.strip_prefix("master/") {
                        (Role::Master, rest)
                    } else if let Some(rest) = path.strip_prefix("slave/") {
                        (Role::Slave, rest)
                    } else {
                        return Err(Errno::ENOENT.into());
                    };
                    (id.parse::<u64>().map_err(|_| Errno::ENOENT)?, role)
                }
            };
            let readable = flags & 1 != 0;
            let writable = flags & 2 != 0;
            if !readable && !writable {
                return Err(Errno::EACCES.into());
            }
            self.pairs.open(pair);
            let id = self.next;
            self.next = self.next.wrapping_add(1);
            self.files.insert(id, FileEntry { pair, role });
            Ok(Opened::File(id))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            let pty = self.pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
            let count = match entry.role {
                Role::Master => pty.read_master(buffer),
                Role::Slave => read_slave_wait(pty, buffer),
            }
            .map_err(raven_scheme::HandlerError::Errno)?;
            Ok(count)
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            let pty = self.pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
            match entry.role {
                Role::Master => pty.write_master(buffer),
                Role::Slave => pty.write_slave(buffer),
            }
            Ok(buffer.len())
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            let pty = self.pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
            if request.len() < raven_proto_tty::CALL_LEN
                || response.len() < raven_proto_tty::CALL_LEN
            {
                return Err(Errno::EINVAL.into());
            }
            let version = u16::from_le_bytes([request[0], request[1]]);
            if version != raven_proto_tty::VERSION {
                return Err(Errno::EINVAL.into());
            }
            let op = u16::from_le_bytes([request[2], request[3]]);
            if matches!(
                op,
                raven_proto_tty::OP_GET_TERMIOS | raven_proto_tty::OP_SET_TERMIOS
            ) && response.len() < raven_proto_tty::TERMIOS_LEN
            {
                return Err(Errno::EINVAL.into());
            }
            if matches!(
                op,
                raven_proto_tty::OP_GET_WINSIZE | raven_proto_tty::OP_SET_WINSIZE
            ) && response.len() < raven_proto_tty::WINSIZE_LEN
            {
                return Err(Errno::EINVAL.into());
            }
            let arg0 = u32::from_le_bytes([request[4], request[5], request[6], request[7]]);
            let arg1 = u32::from_le_bytes([request[8], request[9], request[10], request[11]]);
            match op {
                raven_proto_tty::OP_SET_MODE
                | raven_proto_tty::OP_GET_MODE
                | raven_proto_tty::OP_FLUSH => {
                    let request = decode_request(request).map_err(|_| Errno::EINVAL)?;
                    let mut body = [0u8; raven_proto_tty::CALL_LEN];
                    let reply = match request {
                        Request::SetMode { settings } => {
                            pty.set_settings(settings);
                            Response::Mode(pty.settings())
                        }
                        Request::GetMode => Response::Mode(pty.settings()),
                        Request::Flush => Response::Flushed { bytes: pty.flush() },
                    };
                    encode_response(reply, &mut body);
                    response[..body.len()].copy_from_slice(&body);
                    Ok(body.len())
                }
                raven_proto_tty::OP_GET_TERMIOS | raven_proto_tty::OP_SET_TERMIOS => {
                    match raven_proto_tty::decode_termios_request(request)
                        .map_err(|_| Errno::EINVAL)?
                    {
                        raven_proto_tty::TermiosRequest::Set { termios } => {
                            pty.set_termios(termios);
                        }
                        raven_proto_tty::TermiosRequest::Get => {}
                    }
                    let mut body = [0u8; raven_proto_tty::TERMIOS_LEN];
                    raven_proto_tty::encode_termios_response(&pty.termios(), &mut body);
                    response[..body.len()].copy_from_slice(&body);
                    Ok(body.len())
                }
                raven_proto_tty::OP_GET_WINSIZE | raven_proto_tty::OP_SET_WINSIZE => {
                    match raven_proto_tty::decode_winsize_request(request)
                        .map_err(|_| Errno::EINVAL)?
                    {
                        raven_proto_tty::WinsizeRequest::Set { winsize } => {
                            pty.set_winsize(winsize);
                        }
                        raven_proto_tty::WinsizeRequest::Get => {}
                    }
                    let mut body = [0u8; raven_proto_tty::WINSIZE_LEN];
                    raven_proto_tty::encode_winsize_response(&pty.winsize(), &mut body);
                    response[..body.len()].copy_from_slice(&body);
                    Ok(body.len())
                }
                OP_SET_FG => {
                    pty.set_fg(arg0 as i32);
                    write_custom(response, OP_SET_FG, pty.fg() as u32, 0)
                }
                OP_GET_FG => write_custom(response, OP_GET_FG, pty.fg() as u32, 0),
                OP_HANGUP => {
                    pty.hangup();
                    write_custom(response, OP_HANGUP, 0, 0)
                }
                _ => {
                    let _ = arg1;
                    Err(Errno::ENOSYS.into())
                }
            }
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let entry = self.file(file)?;
            Ok(Stat {
                mode: S_IFCHR | 0o666,
                nlink: 1,
                // The donor devfs pty node has a device inode. The scheme Stat has `ino`, so
                // report the pair/role identity that the SCM_RIGHTS exchange can compare.
                ino: raven_ptyd::inode(entry.pair, entry.role == Role::Slave),
                ..Stat::default()
            })
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            let Some(entry) = self.files.remove(&file) else {
                return Err(Errno::EBADF.into());
            };
            // The kernel delivers CLOSE once the last duplicate of the slave is gone; from
            // then on the master sees EOF instead of an empty read.
            if entry.role == Role::Slave
                && !self
                    .files
                    .values()
                    .any(|other| other.pair == entry.pair && other.role == Role::Slave)
                && let Some(pty) = self.pairs.get_mut(entry.pair)
            {
                pty.hangup();
            }
            // The donor releases the pty unit when its last endpoint closes; a pair that
            // survives every close is a leak that only shows up after many SSH sessions.
            if !self.files.values().any(|other| other.pair == entry.pair) {
                self.pairs.remove(entry.pair);
                if self.files.is_empty() {
                    raven_rt::eprintln!("ptyd: files=0 pairs={}", self.pairs.len());
                }
            }
            Ok(())
        }
    }

    fn write_custom(out: &mut [u8], op: u16, arg0: u32, arg1: u32) -> HandlerResult<usize> {
        out[..2].copy_from_slice(&raven_proto_tty::VERSION.to_le_bytes());
        out[2..4].copy_from_slice(&op.to_le_bytes());
        out[4..8].copy_from_slice(&arg0.to_le_bytes());
        out[8..12].copy_from_slice(&arg1.to_le_bytes());
        Ok(raven_proto_tty::CALL_LEN)
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("pty") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("ptyd: {}", error.description());
                return 1;
            }
        };
        let mut handler = HandlerImpl::new();
        raven_rt::eprintln!("ptyd: files=0 pairs=0");
        loop {
            if let Some(packet) = server.poll(None) {
                let _ = server.dispatch(packet, &mut handler);
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
