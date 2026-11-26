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
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    use raven_abi::errno::Errno;
    use raven_abi::types::clock;
    use raven_proto_fs::{S_IFCHR, S_IFDIR, Stat};
    use raven_proto_tty::{self, Request, Response, decode_request, encode_response};
    use raven_ptyd::{Pairs, SignalSink};
    use raven_rt::entry;
    use raven_rt::sync::Mutex;
    use raven_rt::sys::{clock_get, futex_wait, futex_wake};
    use raven_rt::thread;
    use raven_scheme::{Completer, Ctx, Handler, HandlerError, HandlerResult, Opened, Server};
    use raven_tty_core::{VMIN, VTIME};

    entry!(main);

    const PROC_SIGNAL: usize = 0x4A;
    const PROC_TIMES: usize = 0x4C;
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
            let result = Errno::from_ret(raw).map(|_| ());
            match result {
                Ok(()) => raven_rt::println!("ptyd: signal pgrp={pgrp} signo={signo} ok"),
                Err(error) => {
                    raven_rt::println!("ptyd: signal pgrp={pgrp} signo={signo} errno={error:?}");
                }
            }
            result
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

    /// This process's `(user_ns, system_ns)`.
    ///
    /// `system_ns` is wall-inclusive for a blocking syscall (K508 accounts the whole syscall),
    /// so only `user_ns` is used as an idle-CPU bound; the timer and server wait in the kernel.
    fn cpu_times() -> (u64, u64) {
        let mut values = [0u64; 4];
        // SAFETY: PROC_TIMES writes four u64 words to this live array; the syscall reads only
        // scalar arguments and never retains the pointer.
        let raw = unsafe {
            raven_rt::sys::syscall2(PROC_TIMES, values.as_mut_ptr() as usize, values.len())
        };
        if raw < 0 {
            return (0, 0);
        }
        (values[0], values[1])
    }

    /// One slave read waiting for its own pair. The writer completes it; no worker polls.
    struct DeferredRead {
        token: u64,
        /// Capture buffer mapped by the kernel for this deferred request.
        capture: usize,
        length: usize,
        /// Which end of the pair the blocked read belongs to.
        role: Role,
        /// `VTIME` deadline in nanoseconds; zero means wait for data only.
        vtime_ns: u64,
        start_ns: u64,
        completer: Completer,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Role {
        Master,
        Slave,
        /// The `/dev/pts` mount root: a directory, not a pair endpoint.
        Root,
    }

    /// The scheme `Stat` has no `st_rdev`; the root is a mount point, not a device, so its
    /// inode is outside the `2N+1`/`2N+2` endpoint identity.
    const ROOT_INO: u64 = 0;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct FileEntry {
        pair: u64,
        role: Role,
    }

    struct HandlerImpl {
        pairs: Pairs<TargetSignal>,
        files: BTreeMap<u64, FileEntry>,
        next: u64,
        completer: Option<Completer>,
        /// One queue per pair: a writer completes only its own pair's blocked reads, while the
        /// VTIME timer scans the same map from its own thread.
        deferred: Arc<Mutex<BTreeMap<u64, Vec<DeferredRead>>>>,
        /// Futex generation; the timer sleeps until the earliest VTIME deadline.
        timer_wake: Arc<AtomicU32>,
        timer_wakes: Arc<AtomicU64>,
        /// `(user_ns, wall_ns, timer_wakes)` sampled when the 8th read blocks with VTIME=0.
        idle_start: Option<(u64, u64, u64)>,
    }

    impl HandlerImpl {
        fn new() -> Self {
            Self {
                pairs: Pairs::new(),
                files: BTreeMap::new(),
                next: 1,
                completer: None,
                deferred: Arc::new(Mutex::new(BTreeMap::new())),
                timer_wake: Arc::new(AtomicU32::new(0)),
                timer_wakes: Arc::new(AtomicU64::new(0)),
                idle_start: None,
            }
        }

        fn file(&self, file: u64) -> Result<FileEntry, Errno> {
            self.files.get(&file).copied().ok_or(Errno::EBADF)
        }

        fn wake_timer(&self) {
            self.timer_wake.fetch_add(1, Ordering::AcqRel);
            let _ = futex_wake(self.timer_wake.as_ref(), 1);
        }

        fn defer_read(
            &mut self,
            pair: u64,
            token: u64,
            capture: usize,
            length: usize,
            role: Role,
            vtime: u64,
        ) -> Result<(), Errno> {
            let Some(completer) = self.completer.clone() else {
                return Err(Errno::EAGAIN);
            };
            let vtime_ns = vtime.saturating_mul(100_000_000);
            let (total, all_data_only) = {
                let mut map = self.deferred.lock();
                map.entry(pair).or_default().push(DeferredRead {
                    token,
                    capture,
                    length,
                    role,
                    vtime_ns,
                    start_ns: now_ns(),
                    completer,
                });
                let total: usize = map.values().map(Vec::len).sum();
                let all_data_only = map.values().flatten().all(|job| job.vtime_ns == 0);
                (total, all_data_only)
            };
            if vtime_ns != 0 {
                self.wake_timer();
            }
            if total >= 8 && all_data_only {
                self.idle_start = Some((
                    cpu_times().0,
                    now_ns(),
                    self.timer_wakes.load(Ordering::Acquire),
                ));
            }
            Ok(())
        }

        /// Complete this pair's blocked reads on `role` from data the writer just queued.
        fn complete_pair(&mut self, pair: u64, role: Role) {
            let jobs = {
                let mut map = self.deferred.lock();
                map.remove(&pair)
            };
            let Some(jobs) = jobs else {
                return;
            };
            let mut keep = Vec::new();
            let mut completed = Vec::new();
            let mut has_vtime = false;
            for job in jobs {
                if job.vtime_ns != 0 {
                    has_vtime = true;
                }
                if job.role != role {
                    keep.push(job);
                    continue;
                }
                let Some(pty) = self.pairs.get_mut(pair) else {
                    completed.push((job, Err(Errno::EBADF)));
                    continue;
                };
                // SAFETY: the capture buffer stays mapped until this deferred token is
                // completed; the job was removed from the shared queue under its lock, and
                // the timer cannot see it until a non-empty keep vector is reinserted.
                let buffer =
                    unsafe { core::slice::from_raw_parts_mut(job.capture as *mut u8, job.length) };
                let result = match role {
                    Role::Master => pty.read_master(buffer),
                    Role::Slave => pty.read_slave(buffer),
                    Role::Root => Err(Errno::EISDIR),
                };
                match result {
                    Ok(count) => completed.push((job, Ok(count))),
                    Err(Errno::EAGAIN) => keep.push(job),
                    Err(error) => completed.push((job, Err(error))),
                }
            }
            if !keep.is_empty() {
                self.deferred.lock().insert(pair, keep);
            }
            if has_vtime {
                self.wake_timer();
            }
            for (job, result) in completed {
                let _ = job.completer.complete(job.token, result);
            }
            self.finish_idle();
        }

        /// Complete every blocked read for a pair (hangup/close), leaving no capture dangling.
        ///
        /// A master reader gets `EIO` once the slave side is gone (FreeBSD
        /// `sys/kern/tty_pts.c:223 ptsdev_read`); a slave reader keeps the `0` EOF contract.
        fn complete_all(
            &mut self,
            pair: u64,
            master: Result<usize, Errno>,
            slave: Result<usize, Errno>,
        ) {
            let jobs = {
                let mut map = self.deferred.lock();
                map.remove(&pair)
            };
            let Some(jobs) = jobs else {
                return;
            };
            let has_vtime = jobs.iter().any(|job| job.vtime_ns != 0);
            for job in jobs {
                let result = match job.role {
                    Role::Master => master,
                    Role::Slave | Role::Root => slave,
                };
                let _ = job.completer.complete(job.token, result);
            }
            if has_vtime {
                self.wake_timer();
            }
            self.finish_idle();
        }

        /// Print the measured 8/16-pair idle window once the table has no deferred read.
        fn finish_idle(&mut self) {
            let empty = self.deferred.lock().is_empty();
            if !empty {
                return;
            }
            let Some((user0, wall0, wakes0)) = self.idle_start.take() else {
                return;
            };
            let (user1, _) = cpu_times();
            let wall1 = now_ns();
            let wakes1 = self.timer_wakes.load(Ordering::Acquire);
            raven_rt::eprintln!(
                "ptyd: idle-window wall-ns={} user-ns={} timer-wakes={}",
                wall1.saturating_sub(wall0),
                user1.saturating_sub(user0),
                wakes1.saturating_sub(wakes0)
            );
        }
    }

    struct TimerArgs {
        jobs: Arc<Mutex<BTreeMap<u64, Vec<DeferredRead>>>>,
        wake: Arc<AtomicU32>,
        wakes: Arc<AtomicU64>,
    }

    fn timer_entry(raw: usize) {
        // SAFETY: `raw` is the `Box::into_raw` from `main`; this thread owns it exactly once.
        let args = unsafe { Box::from_raw(raw as *mut TimerArgs) };
        timer_worker(args.jobs, args.wake, args.wakes)
    }

    /// VTIME timer: sleep on the futex until the earliest deadline; no polling loop.
    fn timer_worker(
        jobs: Arc<Mutex<BTreeMap<u64, Vec<DeferredRead>>>>,
        wake: Arc<AtomicU32>,
        wakes: Arc<AtomicU64>,
    ) -> ! {
        loop {
            let expected = wake.load(Ordering::Acquire);
            let now = now_ns();
            let mut expired = Vec::new();
            let mut next = None;
            {
                let mut map = jobs.lock();
                for (_, list) in map.iter_mut() {
                    let mut keep = Vec::new();
                    for job in list.drain(..) {
                        if job.vtime_ns != 0 && now.saturating_sub(job.start_ns) >= job.vtime_ns {
                            expired.push(job);
                        } else {
                            if job.vtime_ns != 0 {
                                let deadline = job.start_ns.saturating_add(job.vtime_ns);
                                next =
                                    Some(next.map_or(deadline, |value: u64| value.min(deadline)));
                            }
                            keep.push(job);
                        }
                    }
                    *list = keep;
                }
                map.retain(|_, list| !list.is_empty());
            }
            for job in expired {
                let _ = job.completer.complete(job.token, Ok(0));
            }
            let timeout = next.map(|deadline| deadline.saturating_sub(now_ns()).max(1));
            let _ = futex_wait(wake.as_ref(), expected, timeout);
            wakes.fetch_add(1, Ordering::Relaxed);
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
                // The scheme root is the `/dev/pts` mount directory. `svcd` also probes a
                // resident scheme with the bare `pty:` path, so the root must open cleanly.
                "" => (0u64, Role::Root),
                // The historical named pair 0 (`docs/pty.md`).
                "master" => (0u64, Role::Master),
                "slave" => (0u64, Role::Slave),
                // `posix_openpt` opens the clone device; the server allocates the unit, then
                // `fstat` gives the caller the pair id through the endpoint inode.
                "ptmx" => (self.pairs.alloc(), Role::Master),
                _ => {
                    // The `/dev/pts` namespace mount maps `/dev/pts/<N>` to `pty:<N>`; the
                    // public slave number names the live pair's slave endpoint (the same inode
                    // `fstat` reports, 2N+2).
                    if let Ok(id) = path.parse::<u64>() {
                        // Only a live pair has a `/dev/pts/<N>` node: the donor devfs installs the
                        // slave node when the master allocates the unit (FreeBSD `pts_alloc`), so
                        // an unallocated number is `ENOENT`, never an implicit new pair.
                        if self.pairs.get_mut(id).is_none() {
                            return Err(Errno::ENOENT.into());
                        }
                        (id, Role::Slave)
                    } else {
                        let (role, id) = if let Some(rest) = path.strip_prefix("master/") {
                            (Role::Master, rest)
                        } else if let Some(rest) = path.strip_prefix("slave/") {
                            (Role::Slave, rest)
                        } else {
                            return Err(Errno::ENOENT.into());
                        };
                        let id = id.parse::<u64>().map_err(|_| Errno::ENOENT)?;
                        // A named endpoint is also a live-pair node: opening `slave/<N>` for a
                        // closed unit must not resurrect the pair (the pty-path release row
                        // stats `/dev/pts/<N>` after the last close).
                        if self.pairs.get_mut(id).is_none() {
                            return Err(Errno::ENOENT.into());
                        }
                        (id, role)
                    }
                }
            };
            let readable = flags & 1 != 0;
            let writable = flags & 2 != 0;
            if !readable && !writable && role != Role::Root {
                return Err(Errno::EACCES.into());
            }
            // The mount root owns no pair; creating pair 0 for it left `files=0 pairs=1` after
            // the first `/dev/pts` stat (the pty-churn final row).
            if role != Role::Root {
                self.pairs.open(pair);
            }
            let id = self.next;
            self.next = self.next.wrapping_add(1);
            self.files.insert(id, FileEntry { pair, role });
            Ok(Opened::File(id))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            if entry.role == Role::Root {
                return Err(Errno::EISDIR.into());
            }
            let pty = self.pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
            let count = match entry.role {
                Role::Master => pty.read_master(buffer),
                Role::Slave => pty.read_slave(buffer),
                Role::Root => return Err(Errno::EISDIR.into()),
            }
            .map_err(HandlerError::Errno)?;
            Ok(count)
        }

        fn read_with(
            &mut self,
            file: u64,
            _offset: u64,
            buffer: &mut [u8],
            ctx: &Ctx,
        ) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            if entry.role == Role::Root {
                return Err(Errno::EISDIR.into());
            }
            if buffer.is_empty() {
                return self.read(file, 0, buffer);
            }
            if entry.role == Role::Master {
                {
                    let pty = self.pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
                    match pty.read_master(buffer) {
                        Ok(count) => return Ok(count),
                        Err(Errno::EAGAIN) => {}
                        Err(error) => return Err(HandlerError::Errno(error)),
                    }
                }
                self.defer_read(
                    entry.pair,
                    ctx.token,
                    buffer.as_mut_ptr() as usize,
                    buffer.len(),
                    Role::Master,
                    0,
                )
                .map_err(HandlerError::Errno)?;
                return Err(HandlerError::Defer(ctx.token));
            }
            let (vmin, vtime) = {
                let pty = self.pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
                match pty.read_slave(buffer) {
                    Ok(count) => return Ok(count),
                    Err(Errno::EAGAIN) => {}
                    Err(error) => return Err(HandlerError::Errno(error)),
                }
                let termios = pty.termios();
                (usize::from(termios.cc(VMIN)), u64::from(termios.cc(VTIME)))
            };
            if vmin == 0 && vtime == 0 {
                return Err(HandlerError::Errno(Errno::EAGAIN));
            }
            self.defer_read(
                entry.pair,
                ctx.token,
                buffer.as_mut_ptr() as usize,
                buffer.len(),
                Role::Slave,
                vtime,
            )
            .map_err(HandlerError::Errno)?;
            Err(HandlerError::Defer(ctx.token))
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            if entry.role == Role::Root {
                return Err(Errno::EISDIR.into());
            }
            {
                let pty = self.pairs.get_mut(entry.pair).ok_or(Errno::EBADF)?;
                match entry.role {
                    Role::Master => pty.write_master(buffer),
                    Role::Slave => pty.write_slave(buffer),
                    Role::Root => return Err(Errno::EISDIR.into()),
                }
            }
            if entry.role == Role::Master {
                self.complete_pair(entry.pair, Role::Slave);
            } else {
                self.complete_pair(entry.pair, Role::Master);
            }
            Ok(buffer.len())
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            let entry = self.file(file)?;
            if entry.role == Role::Root {
                return Err(Errno::ENOTTY.into());
            }
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
            let mut hangup = false;
            let result = match op {
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
                    hangup = true;
                    write_custom(response, OP_HANGUP, 0, 0)
                }
                _ => {
                    let _ = arg1;
                    Err(Errno::ENOSYS.into())
                }
            };
            if hangup {
                self.complete_all(entry.pair, Err(Errno::EIO), Ok(0));
            }
            result
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let entry = self.file(file)?;
            if entry.role == Role::Root {
                // `/dev/pts` is the namespace mount, so its root stats as a directory. A char
                // device here made `stat("/dev/pts")` fail `S_ISDIR` and broke the mount row.
                return Ok(Stat {
                    mode: S_IFDIR | 0o755,
                    nlink: 2,
                    ino: ROOT_INO,
                    ..Stat::default()
                });
            }
            // The slave is the session's tty node: root-owned and 0600, like a devfs pty node
            // after `pty_setowner`. OpenSSH treats a chown/chmod `EROFS` on such a node as a
            // read-only device and continues. The master is opened by the session owner and
            // keeps the permissive device mode.
            let mode = if entry.role == Role::Slave {
                S_IFCHR | 0o600
            } else {
                S_IFCHR | 0o666
            };
            Ok(Stat {
                mode,
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
            if entry.role == Role::Root {
                // The mount root owns no pair, so closing it must not touch the pair table.
                return Ok(());
            }
            // The kernel delivers CLOSE once the last duplicate of the slave is gone; from
            // then on the master sees EIO (FreeBSD tty_pts.c:223) after any queued data. A
            // deferred slave read is completed with 0, so a hanging reader cannot outlive its
            // pair.
            let last_slave = entry.role == Role::Slave
                && !self
                    .files
                    .values()
                    .any(|other| other.pair == entry.pair && other.role == Role::Slave);
            if last_slave {
                if let Some(pty) = self.pairs.get_mut(entry.pair) {
                    pty.hangup();
                }
                self.complete_all(entry.pair, Err(Errno::EIO), Ok(0));
            }
            // The donor releases the pty unit when its last endpoint closes; a pair that
            // survives every close is a leak that only shows up after many SSH sessions.
            if !self.files.values().any(|other| other.pair == entry.pair) {
                self.complete_all(entry.pair, Err(Errno::EIO), Ok(0));
                self.pairs.remove(entry.pair);
                if self.files.is_empty() {
                    let (user, system) = cpu_times();
                    raven_rt::eprintln!("ptyd: files=0 pairs={}", self.pairs.len());
                    raven_rt::eprintln!(
                        "ptyd: timer-wakes={}",
                        self.timer_wakes.load(Ordering::Acquire)
                    );
                    raven_rt::eprintln!("ptyd: user-ns={user}");
                    raven_rt::eprintln!("ptyd: system-ns={system}");
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
        let timer_args = Box::new(TimerArgs {
            jobs: Arc::clone(&handler.deferred),
            wake: Arc::clone(&handler.timer_wake),
            wakes: Arc::clone(&handler.timer_wakes),
        });
        let raw = Box::into_raw(timer_args) as usize;
        if thread::spawn(timer_entry, raw, 32 * 1024).is_err() {
            // SAFETY: the spawn failed, so no thread owns `raw`.
            drop(unsafe { Box::from_raw(raw as *mut TimerArgs) });
            raven_rt::eprintln!("ptyd: timer spawn failed");
            return 1;
        }
        let (user, system) = cpu_times();
        raven_rt::eprintln!("ptyd: files=0 pairs=0");
        raven_rt::eprintln!("ptyd: timer-wakes=0");
        raven_rt::eprintln!("ptyd: user-ns={user}");
        raven_rt::eprintln!("ptyd: system-ns={system}");
        loop {
            if let Some(packet) = server.poll(None) {
                let _ = server.dispatch(packet, &mut handler);
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
