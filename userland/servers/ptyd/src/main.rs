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
    use alloc::vec::Vec;
    use core::sync::atomic::AtomicU32;

    use raven_abi::errno::Errno;
    use raven_abi::types::clock;
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_proto_tty::{self, Request, Response, decode_request, encode_response};
    use raven_ptyd::{Pairs, SignalSink};
    use raven_rt::entry;
    use raven_rt::sys::{clock_get, nanosleep};
    use raven_scheme::{Completer, Ctx, Handler, HandlerError, HandlerResult, Opened, Server};
    use raven_tty_core::{VMIN, VTIME};

    entry!(main);

    const PROC_SIGNAL: usize = 0x4A;
    /// Raw `proc_getrusage(who, out, count)` number; who=0 is this process.
    const PROC_GETRUSAGE: usize = 0x4D;

    /// Idle wait word; the server sleeps here when the S11 receive path returns without a
    /// request instead of busy-spinning on scheduler kicks.
    static IDLE_WORD: AtomicU32 = AtomicU32::new(0);
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

    /// This process's user+system CPU time in nanoseconds, or zero when unavailable.
    fn cpu_ns() -> (u64, u64) {
        let mut values = [0u64; 8];
        // SAFETY: GETRUSAGE writes eight u64 words into the live array and validates the count.
        let raw = unsafe {
            raven_rt::sys::syscall3(
                PROC_GETRUSAGE,
                0,
                values.as_mut_ptr() as usize,
                values.len(),
            )
        };
        if raw < 0 {
            (0, 0)
        } else {
            (values[0], values[1])
        }
    }

    fn print_stats() {
        let (user, system) = cpu_ns();
        raven_rt::eprintln!("ptyd: cpu user-ns={user} sys-ns={system}");
    }

    /// Monotonic nanoseconds, or zero when the clock is unavailable.
    fn now_ns() -> u64 {
        clock_get(clock::MONOTONIC).map_or(0, |time| {
            (time.tv_sec as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(time.tv_nsec as u64)
        })
    }

    /// Synchronous VMIN=0/VTIME read timeout. It keeps the K485 `VMIN=0,VTIME=1` ~100 ms
    /// contract without a timer thread; while it waits it can delay other pairs by at most VTIME.
    fn read_slave_vtime(
        pty: &mut raven_ptyd::Pty<TargetSignal>,
        buffer: &mut [u8],
        vtime: u64,
    ) -> Result<usize, Errno> {
        let start = now_ns();
        let limit = vtime.saturating_mul(100_000_000);
        loop {
            let _ = nanosleep(1_000_000);
            match pty.read_slave(buffer) {
                Ok(count) => return Ok(count),
                Err(Errno::EAGAIN) => {}
                Err(error) => return Err(error),
            }
            if now_ns().saturating_sub(start) >= limit {
                return Ok(0);
            }
        }
    }

    /// A slave read deferred while the server loop services other pairs.
    ///
    /// `ptr`/`len` are the client's captured read buffer in this server's address space. The
    /// kernel keeps the capture mapped until the deferred reply is sent, so the completion path
    /// copies into it before calling `Completer::complete`.
    #[derive(Clone, Copy)]
    struct PendingRead {
        token: u64,
        file: u64,
        pair: u64,
        ptr: usize,
        len: usize,
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
        pending: Vec<PendingRead>,
        next: u64,
    }

    impl HandlerImpl {
        fn new() -> Self {
            Self {
                pairs: Pairs::new(),
                files: BTreeMap::new(),
                pending: Vec::new(),
                next: 1,
            }
        }

        /// Wake deferred slave reads whose data arrived.
        ///
        /// The buffer pointer belongs to the client's capture and is valid until the deferred
        /// reply is sent, so the copy happens before `complete`.
        fn service_pending(&mut self, completer: &Completer) {
            let mut index = 0usize;
            while index < self.pending.len() {
                let pending = self.pending[index];
                if !self.files.contains_key(&pending.file) {
                    self.pending.remove(index);
                    let _ = completer.complete(pending.token, Err(Errno::EBADF));
                    continue;
                }
                let Some(pty) = self.pairs.get_mut(pending.pair) else {
                    self.pending.remove(index);
                    let _ = completer.complete(pending.token, Ok(0));
                    continue;
                };
                // SAFETY: the kernel keeps the read capture mapped for the deferred request; the
                // handler keeps `ptr`/`len` from the original `&mut [u8]` and writes only before
                // completing the request.
                let buffer =
                    unsafe { core::slice::from_raw_parts_mut(pending.ptr as *mut u8, pending.len) };
                match pty.read_slave(buffer) {
                    Ok(count) => {
                        self.pending.remove(index);
                        let _ = completer.complete(pending.token, Ok(count));
                        continue;
                    }
                    Err(Errno::EAGAIN) => {}
                    Err(error) => {
                        self.pending.remove(index);
                        let _ = completer.complete(pending.token, Err(error));
                        continue;
                    }
                }
                index = index.saturating_add(1);
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
            match entry.role {
                Role::Master => pty.read_master(buffer),
                Role::Slave => pty.read_slave(buffer),
            }
            .map_err(raven_scheme::HandlerError::Errno)
        }

        fn read_with(
            &mut self,
            file: u64,
            _offset: u64,
            buffer: &mut [u8],
            ctx: &Ctx,
        ) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            let (result, vmin, vtime) = {
                let pty = self.pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
                let termios = pty.termios();
                let result = match entry.role {
                    Role::Master => pty.read_master(buffer),
                    Role::Slave => pty.read_slave(buffer),
                };
                (
                    result,
                    usize::from(termios.cc(VMIN)),
                    u64::from(termios.cc(VTIME)),
                )
            };
            match result {
                Ok(count) => Ok(count),
                Err(Errno::EAGAIN) if entry.role == Role::Slave && vmin != 0 => {
                    if buffer.is_empty() {
                        return Ok(0);
                    }
                    let pending = PendingRead {
                        token: ctx.token,
                        file,
                        pair: entry.pair,
                        ptr: buffer.as_mut_ptr() as usize,
                        len: buffer.len(),
                    };
                    self.pending.push(pending);
                    raven_rt::eprintln!(
                        "ptyd: read-wait file={file} pair={} vmin={vmin} vtime={vtime}",
                        entry.pair
                    );
                    Err(HandlerError::Defer(ctx.token))
                }
                Err(Errno::EAGAIN) if entry.role == Role::Slave && vtime != 0 => {
                    let pty = self.pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
                    read_slave_vtime(pty, buffer, vtime).map_err(Into::into)
                }
                Err(error) => Err(error.into()),
            }
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
                    print_stats();
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
        let completer = server.completer();
        raven_rt::eprintln!("ptyd: files=0 pairs=0");
        print_stats();
        loop {
            let packet = server.poll(None);
            let had_request = packet.is_some();
            if let Some(packet) = packet {
                let _ = server.dispatch(packet, &mut handler);
            }
            handler.service_pending(&completer);
            if !had_request {
                // The S11 recv path can return without a request under a host-side kick;
                // sleep 50 ms rather than busy-spin the server. A real request is still
                // picked up on the next recv, far below the 500 ms contract.
                let _ = raven_rt::sys::futex_wait(&IDLE_WORD, 0, Some(50_000_000));
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
