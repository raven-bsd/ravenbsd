// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `procd` target server.
//!
//! The server owns one connection file per `OPEN proc:` and dispatches version-1 requests to the
//! host-testable [`raven_procd::Procd`] table. Spawn uses the existing initfs loader; wait and
//! terminate use the kernel process handle. Signals, sessions, exec, limits and accounting are
//! reported as `ENOSYS` until the kernel seam P2f lands.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // The raw process syscalls below have SAFETY comments.

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeSet;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_procd::{ProcInfo, ProcSys, Procd, WaitEvent};
    use raven_proto_fs::OpenFlags;
    use raven_proto_proc::{decode_request, encode_response};
    use raven_rt::entry;
    use raven_rt::spawn::{SpawnError, SpawnOptions, spawn};
    use raven_scheme::{CallReply, Ctx, Handler, HandlerError, HandlerResult, Opened, Server};

    entry!(main);

    const PROC_SELF_INFO: usize = 0x35;
    const PROC_PID_INFO: usize = 0x3A;
    const PROC_SETPGID: usize = 0x36;
    const PROC_HANDLE_INFO: usize = 0x3E;
    const PROC_WAIT_EX: usize = 0x48;
    const PROC_GETPGID: usize = 0x49;
    const PROC_SIGNAL: usize = 0x4A;

    struct TargetSys {
        initfs: Initfs<'static>,
        self_pid: i32,
    }

    impl ProcSys for TargetSys {
        fn spawn(&mut self, path: &str, argv: &[&str], env: &[&str]) -> Result<Handle, Errno> {
            spawn(&self.initfs, path, &SpawnOptions { args: argv, env }).map_err(spawn_error)
        }

        fn pid(&mut self, handle: Handle) -> Result<i32, Errno> {
            // SAFETY: PROC_HANDLE_INFO takes one scalar handle value; the kernel validates it.
            let raw = unsafe { raven_rt::sys::syscall1(PROC_HANDLE_INFO, handle.0 as usize) };
            Errno::from_ret(raw).and_then(|pid| i32::try_from(pid).map_err(|_| Errno::EOVERFLOW))
        }

        fn info(&mut self, pid: i32) -> Result<ProcInfo, Errno> {
            let mut words = [0u64; 8];
            // SAFETY: PID_INFO takes a signed pid and a live eight-word array owned by the
            // caller; the kernel validates the pid and writes exactly eight u64 values.
            let raw = unsafe {
                raven_rt::sys::syscall3(
                    PROC_PID_INFO,
                    pid as isize as usize,
                    words.as_mut_ptr() as usize,
                    words.len(),
                )
            };
            Errno::from_ret(raw)?;
            let value = |index: usize| -> Result<i32, Errno> {
                let word = words.get(index).copied().ok_or(Errno::EINVAL)?;
                i32::try_from(word).map_err(|_| Errno::EOVERFLOW)
            };
            Ok(ProcInfo {
                pid: value(0)?,
                ppid: value(1)?,
                pgid: value(2)?,
                sid: value(3)?,
            })
        }

        fn wait_ex(&mut self, idtype: u32, id: i32, options: u32) -> Result<WaitEvent, Errno> {
            let mut status = 0i32;
            // SAFETY: WAIT_EX takes three scalar selectors and a writable i32 status pointer
            // owned by the caller; the kernel validates the idtype, the target and the range.
            let raw = unsafe {
                raven_rt::sys::syscall4(
                    PROC_WAIT_EX,
                    idtype as usize,
                    id as isize as usize,
                    options as usize,
                    &mut status as *mut i32 as usize,
                )
            };
            let pid = Errno::from_ret(raw)?;
            Ok(WaitEvent {
                pid: i32::try_from(pid).map_err(|_| Errno::EOVERFLOW)?,
                status,
            })
        }

        fn signal(&mut self, target: i32, signo: u32) -> Result<(), Errno> {
            // SAFETY: SIGNAL takes two scalar values; the kernel validates the target, signal
            // number, uid permission and group membership.
            let raw = unsafe {
                raven_rt::sys::syscall2(PROC_SIGNAL, target as isize as usize, signo as usize)
            };
            Errno::from_ret(raw).map(|_| ())
        }

        fn getpgid(&mut self, pid: i32) -> Result<i32, Errno> {
            // SAFETY: GETPGID takes one scalar pid; the kernel validates it and returns the
            // stored group id.
            let raw = unsafe { raven_rt::sys::syscall1(PROC_GETPGID, pid as isize as usize) };
            Errno::from_ret(raw).and_then(|pgid| i32::try_from(pgid).map_err(|_| Errno::EOVERFLOW))
        }

        fn setpgid(&mut self, pid: i32, pgid: i32) -> Result<(), Errno> {
            // SAFETY: PROC_SETPGID takes two scalar signed ids; the kernel checks the target,
            // the caller's child relationship and the session/group rules.
            let raw = unsafe {
                raven_rt::sys::syscall2(PROC_SETPGID, pid as isize as usize, pgid as isize as usize)
            };
            Errno::from_ret(raw).map(|_| ())
        }

        fn close(&mut self, handle: Handle) {
            let _ = raven_rt::sys::handle_close(handle);
        }

        fn self_pid(&mut self) -> Result<i32, Errno> {
            Ok(self.self_pid)
        }
    }

    struct HandlerImpl {
        procd: Procd<TargetSys>,
        files: BTreeSet<u64>,
        next_file: u64,
    }

    impl HandlerImpl {
        fn new(procd: Procd<TargetSys>) -> Self {
            Self {
                procd,
                files: BTreeSet::new(),
                next_file: 1,
            }
        }
    }

    impl Handler for HandlerImpl {
        fn open_with(
            &mut self,
            path: &str,
            flags: OpenFlags,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            if !path.is_empty() && path != "control" {
                return Err(Errno::ENOENT.into());
            }
            if flags.create() || flags.directory() {
                return Err(Errno::EINVAL.into());
            }
            let id = self.next_file;
            self.next_file = self.next_file.wrapping_add(1).max(1);
            self.files.insert(id);
            Ok(Opened::File(id))
        }

        fn call_with(
            &mut self,
            file: u64,
            request: &[u8],
            response: &mut [u8],
            ctx: &Ctx,
        ) -> HandlerResult<CallReply> {
            if !self.files.contains(&file) {
                return Err(Errno::EBADF.into());
            }
            let (_header, request) = decode_request(request).map_err(HandlerError::Errno)?;
            let reply = self.procd.dispatch(ctx.credentials.euid, request);
            let written = encode_response(&reply, response).map_err(HandlerError::Errno)?;
            Ok(CallReply::new(written))
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            if self.files.remove(&file) {
                Ok(())
            } else {
                Err(Errno::EBADF.into())
            }
        }
    }

    fn spawn_error(error: SpawnError) -> Errno {
        match error {
            SpawnError::Sys(error) => error,
            SpawnError::NotFound => Errno::ENOENT,
            SpawnError::FileNotFound => Errno::ENOENT,
            SpawnError::BadElf(_) => Errno::ENOEXEC,
            _ => Errno::EINVAL,
        }
    }

    fn self_pid() -> Result<i32, Errno> {
        let mut words = [0u64; 8];
        // SAFETY: SELF_INFO takes a live eight-word array owned by the caller; the kernel writes
        // exactly eight u64 identity values.
        let raw = unsafe {
            raven_rt::sys::syscall2(PROC_SELF_INFO, words.as_mut_ptr() as usize, words.len())
        };
        Errno::from_ret(raw)?;
        let value = words.first().copied().ok_or(Errno::EINVAL)?;
        i32::try_from(value).map_err(|_| Errno::EOVERFLOW)
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let image = match raven_rt::map_initfs() {
            Ok(image) => image,
            Err(error) => {
                raven_rt::eprintln!("procd: initfs: {}", error.description());
                return 1;
            }
        };
        let initfs = match Initfs::parse(image) {
            Ok(initfs) => initfs,
            Err(error) => {
                raven_rt::eprintln!("procd: initfs parse: {error}");
                return 1;
            }
        };
        let pid = match self_pid() {
            Ok(pid) => pid,
            Err(error) => {
                raven_rt::eprintln!("procd: self-info: {}", error.description());
                return 1;
            }
        };
        let mut server = match Server::register("proc") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("procd: register: {}", error.description());
                return 1;
            }
        };
        log_started("procd");
        let mut handler = HandlerImpl::new(Procd::new(
            TargetSys {
                initfs,
                self_pid: pid,
            },
            pid,
        ));
        loop {
            if let Some(packet) = server.poll(None) {
                let _ = server.dispatch(packet, &mut handler);
            }
        }
    }

    fn log_started(name: &str) {
        let client = raven_scheme::Client::new();
        let path = alloc::format!("log:{name}");
        if let Ok(handle) = client.open(&path, 2) {
            let _ = client.write(handle, 0, b"started\n");
            let _ = client.close(handle);
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
