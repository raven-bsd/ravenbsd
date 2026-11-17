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
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use core::fmt::Write as _;

    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, Rights};
    use raven_initfs::Initfs;
    use raven_procd::{ProcInfo, ProcSys, Procd, WaitEvent, command_name};
    use raven_proto_fs::{OpenFlags, Stat};
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
            let handle = spawn(&self.initfs, path, &SpawnOptions { args: argv, env })
                .map_err(spawn_error)?;
            name_child(handle, argv, path)?;
            Ok(handle)
        }

        fn spawn_with_attributes(
            &mut self,
            path: &str,
            argv: &[&str],
            env: &[&str],
            fdmap: &[(i32, u32)],
            sigmask: &[u8; 128],
            sigdefault: &[u8; 128],
            ignored: &[u8; 128],
        ) -> Result<Handle, Errno> {
            let attributes = raven_rt::spawn::SpawnAttributes {
                fdmap,
                sigmask,
                sigdefault,
                ignored,
            };
            raven_rt::spawn::spawn_with_attributes(
                &self.initfs,
                path,
                &SpawnOptions { args: argv, env },
                &attributes,
            )
            .map_err(spawn_error)
            .and_then(|handle| {
                name_child(handle, argv, path)?;
                Ok(handle)
            })
        }

        fn validate_handle(&mut self, handle: Handle) -> Result<(), Errno> {
            // A transfer into the server itself proves the descriptor is live and carries the
            // TRANSFER right without creating the child yet. Close the probe copy immediately.
            let probe = raven_rt::sys::handle_transfer(Handle::SELF_PROC, handle, Rights::ALL)?;
            let _ = raven_rt::sys::handle_close(probe);
            Ok(())
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

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FileKind {
        Control,
        Registry,
    }

    struct HandlerImpl {
        procd: Procd<TargetSys>,
        files: BTreeMap<u64, FileKind>,
        next_file: u64,
    }

    impl HandlerImpl {
        fn new(procd: Procd<TargetSys>) -> Self {
            Self {
                procd,
                files: BTreeMap::new(),
                next_file: 1,
            }
        }

        /// One `pid ppid pgid sid uid command` line per child.
        fn registry_text(&self) -> String {
            let mut text = String::new();
            for entry in self.procd.registry() {
                let _ = writeln!(text, "{entry}");
            }
            text
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
            let path = path.strip_prefix('/').unwrap_or(path);
            let kind = match path {
                "" | "control" => FileKind::Control,
                "registry" => FileKind::Registry,
                _ => return Err(Errno::ENOENT.into()),
            };
            if flags.create() || flags.directory() {
                return Err(Errno::EINVAL.into());
            }
            let id = self.next_file;
            self.next_file = self.next_file.wrapping_add(1).max(1);
            self.files.insert(id, kind);
            Ok(Opened::File(id))
        }

        fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            if self.files.get(&file) != Some(&FileKind::Registry) {
                return Err(Errno::EBADF.into());
            }
            let text = self.registry_text();
            let bytes = text.as_bytes();
            let start = usize::try_from(offset).unwrap_or(usize::MAX);
            let Some(available) = bytes.get(start..) else {
                return Ok(0);
            };
            let length = available.len().min(buffer.len());
            if let Some(destination) = buffer.get_mut(..length) {
                destination.copy_from_slice(&available[..length]);
            }
            Ok(length)
        }

        fn write(&mut self, _file: u64, _offset: u64, _buffer: &[u8]) -> HandlerResult<usize> {
            Err(Errno::EBADF.into())
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let Some(kind) = self.files.get(&file).copied() else {
                return Err(Errno::EBADF.into());
            };
            let (size, mode) = match kind {
                FileKind::Control => (0, 0o020666),
                FileKind::Registry => (
                    u64::try_from(self.registry_text().len()).unwrap_or(0),
                    0o100444,
                ),
            };
            Ok(Stat {
                ino: file,
                size,
                mode,
                nlink: 1,
                ..Stat::default()
            })
        }

        fn call_with(
            &mut self,
            file: u64,
            request: &[u8],
            response: &mut [u8],
            ctx: &Ctx,
        ) -> HandlerResult<CallReply> {
            if self.files.get(&file) != Some(&FileKind::Control) {
                return Err(Errno::EBADF.into());
            }
            let (_header, request) = decode_request(request).map_err(HandlerError::Errno)?;
            let reply = self.procd.dispatch(ctx.credentials.euid, request);
            let written = encode_response(&reply, response).map_err(HandlerError::Errno)?;
            Ok(CallReply::new(written))
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            if self.files.remove(&file).is_some() {
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

    /// Name the child before returning its handle to the model.
    fn name_child(handle: Handle, argv: &[&str], path: &str) -> Result<(), Errno> {
        let command = command_name(argv, path);
        if let Err(error) = raven_rt::proc::set_command(handle, &command) {
            let _ = raven_rt::sys::proc_terminate(handle, 1);
            let _ = raven_rt::sys::handle_close(handle);
            return Err(error);
        }
        Ok(())
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
