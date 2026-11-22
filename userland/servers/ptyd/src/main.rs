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
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicU64, Ordering};

    use raven_abi::errno::Errno;
    use raven_abi::types::clock;
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_proto_tty::{self, Request, Response, decode_request, encode_response};
    use raven_ptyd::{Pairs, SignalSink};
    use raven_rt::entry;
    use raven_rt::sync::Mutex;
    use raven_rt::sys::{clock_get, nanosleep};
    use raven_rt::thread;
    use raven_scheme::{Completer, Ctx, Handler, HandlerError, HandlerResult, Opened, Server};
    use raven_tty_core::{VMIN, VTIME};

    entry!(main);

    const PROC_SIGNAL: usize = 0x4A;
    const OP_SET_FG: u16 = 0x10;
    const OP_GET_FG: u16 = 0x11;
    const OP_HANGUP: u16 = 0x12;

    /// Total deferred-reader poll iterations (1 ms apart), the idle-cost signal.
    static READER_WAKES: AtomicU64 = AtomicU64::new(0);

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

    /// One deferred slave read with its own worker: a blocked read on one pair cannot delay the
    /// completion of another pair.
    struct ReadJob {
        token: u64,
        pairs: Arc<Mutex<Pairs<TargetSignal>>>,
        pair: u64,
        capture: usize,
        length: usize,
        vtime: u64,
        start: u64,
        completer: Completer,
    }

    /// Per-read worker: poll its own pair, write the captured buffer, complete exactly once.
    fn read_job(raw: usize) {
        // SAFETY: `raw` is the `Box::into_raw` from `read_with`; this worker reclaims it once.
        let job = unsafe { Box::from_raw(raw as *mut ReadJob) };
        let limit = (job.vtime != 0).then_some(job.vtime.saturating_mul(100_000_000));
        loop {
            let result = {
                let mut pairs = job.pairs.lock();
                match pairs.get_mut(job.pair) {
                    Some(pty) => {
                        // SAFETY: the deferred capture stays mapped until `complete`; the slice
                        // length is the caller's captured buffer length.
                        let buffer = unsafe {
                            core::slice::from_raw_parts_mut(job.capture as *mut u8, job.length)
                        };
                        Some(pty.read_slave(buffer))
                    }
                    None => None,
                }
            };
            match result {
                Some(Ok(count)) => {
                    let _ = job.completer.complete(job.token, Ok(count));
                    return;
                }
                Some(Err(Errno::EAGAIN)) => {}
                Some(Err(error)) => {
                    let _ = job.completer.complete(job.token, Err(error));
                    return;
                }
                None => {
                    let _ = job.completer.complete(job.token, Err(Errno::EBADF));
                    return;
                }
            }
            if let Some(limit) = limit
                && now_ns().saturating_sub(job.start) >= limit
            {
                let _ = job.completer.complete(job.token, Ok(0));
                return;
            }
            READER_WAKES.fetch_add(1, Ordering::Relaxed);
            let _ = nanosleep(1_000_000);
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
        pairs: Arc<Mutex<Pairs<TargetSignal>>>,
        files: BTreeMap<u64, FileEntry>,
        next: u64,
        completer: Option<Completer>,
    }

    impl HandlerImpl {
        fn new() -> Self {
            Self {
                pairs: Arc::new(Mutex::new(Pairs::new())),
                files: BTreeMap::new(),
                next: 1,
                completer: None,
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
                "ptmx" => (self.pairs.lock().alloc(), Role::Master),
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
            self.pairs.lock().open(pair);
            let id = self.next;
            self.next = self.next.wrapping_add(1);
            self.files.insert(id, FileEntry { pair, role });
            Ok(Opened::File(id))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            let mut pairs = self.pairs.lock();
            let pty = pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
            match entry.role {
                Role::Master => pty
                    .read_master(buffer)
                    .map_err(raven_scheme::HandlerError::Errno),
                Role::Slave => pty
                    .read_slave(buffer)
                    .map_err(raven_scheme::HandlerError::Errno),
            }
        }

        fn read_with(
            &mut self,
            file: u64,
            _offset: u64,
            buffer: &mut [u8],
            ctx: &Ctx,
        ) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            if entry.role == Role::Master || buffer.is_empty() {
                return self.read(file, 0, buffer);
            }
            let (vtime, vmin) = {
                let mut pairs = self.pairs.lock();
                let pty = pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
                match pty.read_slave(buffer) {
                    Ok(count) => return Ok(count),
                    Err(Errno::EAGAIN) => {}
                    Err(error) => return Err(HandlerError::Errno(error)),
                }
                let termios = pty.termios();
                (u64::from(termios.cc(VTIME)), usize::from(termios.cc(VMIN)))
            };
            if vmin == 0 && vtime == 0 {
                return Err(HandlerError::Errno(Errno::EAGAIN));
            }
            let Some(completer) = self.completer.clone() else {
                return Err(HandlerError::Errno(Errno::EAGAIN));
            };
            let job = Box::new(ReadJob {
                token: ctx.token,
                pairs: Arc::clone(&self.pairs),
                pair: entry.pair,
                capture: buffer.as_mut_ptr() as usize,
                length: buffer.len(),
                vtime,
                start: now_ns(),
                completer,
            });
            let raw = Box::into_raw(job) as usize;
            if thread::spawn(read_job, raw, 32 * 1024).is_err() {
                // SAFETY: the spawn failed, so no worker owns `raw`.
                drop(unsafe { Box::from_raw(raw as *mut ReadJob) });
                return Err(HandlerError::Errno(Errno::EAGAIN));
            }
            Err(HandlerError::Defer(ctx.token))
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            {
                let mut pairs = self.pairs.lock();
                let pty = pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
                match entry.role {
                    Role::Master => pty.write_master(buffer),
                    Role::Slave => pty.write_slave(buffer),
                }
            }
            Ok(buffer.len())
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            let mut pairs = self.pairs.lock();
            let pty = pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
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
            let mut pairs = self.pairs.lock();
            // The kernel delivers CLOSE once the last duplicate of the slave is gone; from
            // then on the master sees EOF instead of an empty read.
            if entry.role == Role::Slave
                && !self
                    .files
                    .values()
                    .any(|other| other.pair == entry.pair && other.role == Role::Slave)
                && let Some(pty) = pairs.get_mut(entry.pair)
            {
                pty.hangup();
            }
            // The donor releases the pty unit when its last endpoint closes; a pair that
            // survives every close is a leak that only shows up after many SSH sessions.
            if !self.files.values().any(|other| other.pair == entry.pair) {
                pairs.remove(entry.pair);
                if self.files.is_empty() {
                    raven_rt::eprintln!("ptyd: files=0 pairs={}", pairs.len());
                    raven_rt::eprintln!(
                        "ptyd: reader-wakes={}",
                        READER_WAKES.load(Ordering::Relaxed)
                    );
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
        handler.completer = Some(server.completer());
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
