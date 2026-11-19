// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P9a: the native `file:` image spawner and the pid -> process-handle table.
//!
//! `posix_spawn` takes a `file:` image path, not the initfs. The spawner reads the image, runs the
//! shared `raven-spawn` loader extracted from `raven-rt`, turns the returned process handle into a
//! pid with `PROC_HANDLE_INFO` (P9b), and records the pair so `waitpid` can call `PROC_WAIT` and
//! encode the Linux wait status.
//!
//! The kernel primitives are behind [`Loader`] and [`HandleInfo`] so host tests exercise the
//! policy without executing a syscall. On the target [`NATIVE_SPAWNER`] wires the real loader.

use alloc::string::String;
use alloc::vec::Vec;

use core::sync::atomic::{AtomicU64, Ordering};

use crate::backend::{EarlySpawn, WaitResult};
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use crate::errors::{E2BIG, ENOENT, ENOEXEC};
use crate::errors::{EAGAIN, EINVAL, ENAMETOOLONG, Errno};
use crate::request::SpawnRequest;
use crate::wait_status::kernel_wait_options;

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_abi::errno::Errno as AbiErrno;
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_abi::types::Handle;

/// Maximum command bytes accepted by the kernel `PROC_SET_COMMAND`.
#[cfg(any(target_os = "none", target_os = "ravenbsd", test))]
const MAX_COMMAND: usize = 256;

/// Maximum number of children tracked at once.
pub const MAX_CHILDREN: usize = 128;

/// One free table slot.
const EMPTY: u64 = 0;
/// A slot that a spawner has claimed but not published yet.
const RESERVED: u64 = u64::MAX;

/// The pid -> handle table slot.
///
/// One `u64` holds `pid << 32 | handle`; zero is free and `u64::MAX` is a reservation. The table
/// is a fixed array of atomics so a `NativeSpawner` can live in a `static` without a lock or a
/// heap allocation.
type Slot = AtomicU64;

/// Read an image and start its first thread.
///
/// The real implementation lives in `raven-spawn`; host tests supply a fake.
pub trait Loader {
    /// Read one executable image into a bounded buffer.
    fn read_image(&self, path: &str) -> Result<Vec<u8>, Errno>;
    /// Load `image` and start the first thread; return the child process handle.
    fn load(
        &self,
        image: &[u8],
        path: &str,
        argv: &[&str],
        env: &[&str],
        cwd: &[u8],
        fdmap: &[(i32, u32)],
    ) -> Result<u32, Errno>;
    /// Wait for a child; a `WNOHANG` miss returns pid 0.
    fn wait_ex(&self, idtype: u32, id: i32, options: u32) -> Result<(i32, i32), Errno>;
    /// Close one process handle.
    fn close(&self, handle: u32) -> Result<(), Errno>;
}

/// Turn a process handle into a pid (`PROC_HANDLE_INFO`, supplied by P9b).
pub trait HandleInfo {
    /// Map one live handle owned by the caller.
    fn pid_for_handle(&self, handle: u32) -> Result<i32, Errno>;
}

/// The native spawner: loader, handle conversion and the pid -> handle table.
pub struct NativeSpawner<L, H> {
    loader: L,
    info: H,
    children: [Slot; MAX_CHILDREN],
}

impl<L, H> NativeSpawner<L, H> {
    /// Create a spawner over one loader and one handle-info provider.
    #[must_use]
    pub const fn new(loader: L, info: H) -> Self {
        Self {
            loader,
            info,
            children: [const { AtomicU64::new(EMPTY) }; MAX_CHILDREN],
        }
    }

    /// Claim one free table slot before starting a child.
    fn reserve(&self) -> Result<&Slot, Errno> {
        for slot in &self.children {
            if slot
                .compare_exchange(EMPTY, RESERVED, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(slot);
            }
        }
        Err(EAGAIN)
    }

    /// Publish a started child into its reserved slot.
    fn publish(slot: &Slot, pid: i32, handle: u32) {
        let pid = pid as u32;
        let value = (u64::from(pid) << 32) | u64::from(handle);
        slot.store(value, Ordering::Release);
    }

    /// Claim the slot for `pid` and return its handle.
    fn take(&self, pid: i32) -> Option<u32> {
        let pid = pid as u32;
        for slot in &self.children {
            let value = slot.load(Ordering::Acquire);
            if value == EMPTY || value == RESERVED || value >> 32 != u64::from(pid) {
                continue;
            }
            if slot
                .compare_exchange(value, EMPTY, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Some(value as u32);
            }
        }
        None
    }
}

impl<L: Loader, H: HandleInfo> EarlySpawn for NativeSpawner<L, H> {
    fn spawn(&self, request: &SpawnRequest) -> Result<i32, Errno> {
        let path = core::str::from_utf8(&request.path).map_err(|_| EINVAL)?;
        if request.cwd.is_empty() || !request.cwd.starts_with(b"/") {
            return Err(EINVAL);
        }
        if request.cwd.len() >= 4096 {
            return Err(ENAMETOOLONG);
        }
        let resolved = resolve_image_path(path, &request.cwd)?;
        let argv = strings(&request.argv)?;
        let envp = strings(&request.envp)?;
        let slot = self.reserve()?;
        let result = (|| {
            let image = self.loader.read_image(&resolved)?;
            let fds: Vec<(i32, u32)> = request
                .fdmap
                .iter()
                .map(|entry| (entry.fd, entry.handle))
                .collect();
            let handle = self
                .loader
                .load(&image, &resolved, &argv, &envp, &request.cwd, &fds)?;
            let pid = match self.info.pid_for_handle(handle) {
                Ok(pid) => pid,
                Err(error) => {
                    let _ = self.loader.close(handle);
                    return Err(error);
                }
            };
            Ok((pid, handle))
        })();
        match result {
            Ok((pid, handle)) => {
                Self::publish(slot, pid, handle);
                Ok(pid)
            }
            Err(error) => {
                slot.store(EMPTY, Ordering::Release);
                Err(error)
            }
        }
    }

    fn wait(&self, idtype: u32, id: i64, options: u32) -> Result<WaitResult, Errno> {
        let id = i32::try_from(id).map_err(|_| EINVAL)?;
        let (pid, status) = self
            .loader
            .wait_ex(idtype, id, kernel_wait_options(options))?;
        if pid != 0
            && let Some(handle) = self.take(pid)
        {
            let _ = self.loader.close(handle);
        }
        Ok(WaitResult {
            pid,
            status,
            siginfo: [0; 128],
            rusage: [0; 144],
        })
    }
}

/// Resolve a spawn path against the parent's `cwd`.
fn resolve_image_path(path: &str, cwd: &[u8]) -> Result<String, Errno> {
    let scheme = match path.find(':') {
        Some(index) => !path[..index].contains('/'),
        None => false,
    };
    if scheme || path.starts_with('/') {
        return Ok(path.into());
    }
    let cwd = core::str::from_utf8(cwd).map_err(|_| EINVAL)?;
    if cwd.is_empty() || cwd == "/" {
        Ok(alloc::format!("/{path}"))
    } else {
        Ok(alloc::format!("{cwd}/{path}"))
    }
}

/// Convert the request byte strings into the loader's UTF-8 argument view.
fn strings(items: &[Vec<u8>]) -> Result<Vec<&str>, Errno> {
    let mut view = Vec::with_capacity(items.len());
    for item in items {
        view.push(core::str::from_utf8(item).map_err(|_| EINVAL)?);
    }
    Ok(view)
}

/// Translate a loader error into a C `errno` value.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
fn map_spawn_error(error: raven_spawn::SpawnError) -> Errno {
    use raven_spawn::SpawnError;
    match error {
        SpawnError::NotFound | SpawnError::FileNotFound => ENOENT,
        SpawnError::BadElf(_) | SpawnError::HasInterpreter => ENOEXEC,
        SpawnError::TooLarge => E2BIG,
        SpawnError::BadCwd => EINVAL,
        SpawnError::Sys(error) => i32::from(error.0),
    }
}

/// Choose and truncate the kernel command name for a native spawn.
///
/// `argv[0]` wins; an empty/missing argument list uses the image path. The result is capped at
/// [`MAX_COMMAND`] bytes at a UTF-8 boundary.
#[cfg(any(target_os = "none", target_os = "ravenbsd", test))]
fn command_name<'a>(argv: &[&'a str], path: &'a str) -> &'a str {
    let mut text = argv.first().copied().unwrap_or(path);
    if text.is_empty() {
        text = path;
    }
    let mut end = 0usize;
    for (index, character) in text.char_indices() {
        let next = index.saturating_add(character.len_utf8());
        if next > MAX_COMMAND {
            break;
        }
        end = next;
    }
    &text[..end]
}

/// A syscall sink that names the child before its first thread starts.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
struct CommandSys<'a> {
    inner: raven_spawn::RtSys,
    command: &'a str,
}

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
impl raven_spawn::Sys for CommandSys<'_> {
    fn proc_create(&mut self) -> Result<Handle, AbiErrno> {
        self.inner.proc_create()
    }

    fn memobj_create(&mut self, len: usize) -> Result<Handle, AbiErrno> {
        self.inner.memobj_create(len)
    }

    fn map(
        &mut self,
        proc: Handle,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, AbiErrno> {
        self.inner.map(proc, memobj, offset, addr, len, flags)
    }

    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), AbiErrno> {
        self.inner.unmap(proc, addr, len)
    }

    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), AbiErrno> {
        self.inner.protect(proc, addr, len, protection)
    }

    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, AbiErrno> {
        raven_libc_sys::sys::proc_set_command(proc, self.command.as_bytes())?;
        self.inner.thread_create(proc, entry, stack_top, arg)
    }

    fn handle_transfer(&mut self, dest: Handle, source: Handle) -> Result<Handle, AbiErrno> {
        self.inner.handle_transfer(dest, source)
    }

    fn exec_cred(&mut self, proc: Handle, file: Handle, nosuid: bool) -> Result<(), AbiErrno> {
        self.inner.exec_cred(proc, file, nosuid)
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), AbiErrno> {
        self.inner.handle_close(handle)
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        self.inner.write_parent_mem(addr, bytes);
    }

    fn copy_mapping(&mut self, destination: usize, source: usize, len: usize) {
        self.inner.copy_mapping(destination, source, len);
    }
}

/// The target loader: `raven-spawn` over the real syscalls.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct RtLoader;

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
impl Loader for RtLoader {
    fn read_image(&self, path: &str) -> Result<Vec<u8>, Errno> {
        raven_spawn::read_file_image(path).map_err(map_spawn_error)
    }

    fn load(
        &self,
        image: &[u8],
        path: &str,
        argv: &[&str],
        env: &[&str],
        cwd: &[u8],
        fdmap: &[(i32, u32)],
    ) -> Result<u32, Errno> {
        let options = raven_spawn::SpawnOptions { args: argv, env };
        let mut sys = CommandSys {
            inner: raven_spawn::RtSys,
            command: command_name(argv, path),
        };
        raven_spawn::spawn_image_with_fdmap_cwd(image, path, &options, cwd, fdmap, &mut sys)
            .map(|handle| handle.0)
            .map_err(map_spawn_error)
    }

    fn wait_ex(&self, idtype: u32, id: i32, options: u32) -> Result<(i32, i32), Errno> {
        raven_spawn::wait_ex(idtype, id, options).map_err(|error| i32::from(error.0))
    }

    fn close(&self, handle: u32) -> Result<(), Errno> {
        raven_spawn::close_handle(handle).map_err(|error| i32::from(error.0))
    }
}

/// The target `PROC_HANDLE_INFO` provider (P9b, raw `0x3E`).
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct RtHandleInfo;

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
impl HandleInfo for RtHandleInfo {
    fn pid_for_handle(&self, handle: u32) -> Result<i32, Errno> {
        raven_spawn::pid_for_handle(handle).map_err(|error| i32::from(error.0))
    }
}

/// The process-wide native spawner used by the target `ProcBackend`.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
pub static NATIVE_SPAWNER: NativeSpawner<RtLoader, RtHandleInfo> =
    NativeSpawner::new(RtLoader, RtHandleInfo);

#[cfg(test)]
#[path = "native_spawn_ut.rs"]
mod tests;
