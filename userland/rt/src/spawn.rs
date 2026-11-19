// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Create a child process from an ELF file in the initfs or on a file server.
//!
//! P9a moved the loader into the shared `raven-spawn` crate so `raven-rt` and ravenlibc use one
//! planning, mapping and stack-image implementation. This module keeps the runtime's public
//! `raven_rt::spawn` surface as a re-export.
//!
//! The re-export still includes the PG-49 `PT_INTERP` mapping and the PG-50 auxiliary vector
//! (`AT_BASE` for an `ET_DYN` interpreter, `AT_PHDR`/`AT_PHNUM` for the main image); the code
//! lives in `crates/raven-spawn/src/lib.rs`.

pub use raven_spawn::*;

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_abi::errno::Errno;
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_abi::types::Handle;

/// Choose and truncate the kernel command name for a spawn.
///
/// The first argument wins; an empty argument list uses the image path. The result is capped at
/// [`crate::proc::MAX_COMMAND`] bytes at a UTF-8 boundary.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
pub(crate) fn command_name<'a>(
    path: &'a str,
    options: &'a raven_spawn::SpawnOptions<'a>,
) -> &'a str {
    let text = options.args.first().copied().unwrap_or(path);
    let mut end = 0usize;
    for (index, character) in text.char_indices() {
        let next = index.saturating_add(character.len_utf8());
        if next > crate::proc::MAX_COMMAND {
            break;
        }
        end = next;
    }
    &text[..end]
}

/// A syscall sink that names the child before its first thread starts.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
struct NamedSys<'a> {
    inner: raven_spawn::RtSys,
    command: &'a str,
}

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
impl raven_spawn::Sys for NamedSys<'_> {
    fn proc_create(&mut self) -> Result<Handle, Errno> {
        self.inner.proc_create()
    }

    fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
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
    ) -> Result<usize, Errno> {
        self.inner.map(proc, memobj, offset, addr, len, flags)
    }

    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
        self.inner.unmap(proc, addr, len)
    }

    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), Errno> {
        self.inner.protect(proc, addr, len, protection)
    }

    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, Errno> {
        crate::proc::set_command(proc, self.command)?;
        self.inner.thread_create(proc, entry, stack_top, arg)
    }

    fn exec_cred(&mut self, proc: Handle, file: Handle, nosuid: bool) -> Result<(), Errno> {
        self.inner.exec_cred(proc, file, nosuid)
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
        self.inner.handle_close(handle)
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        self.inner.write_parent_mem(addr, bytes);
    }

    fn copy_mapping(&mut self, destination: usize, source: usize, len: usize) {
        self.inner.copy_mapping(destination, source, len);
    }
}

/// Spawn an initfs image and name the child before its first thread starts.
///
/// # Errors
/// [`SpawnError`] from the loader or a rejected `PROC_SET_COMMAND`.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
pub fn spawn(
    initfs: &raven_initfs::Initfs<'_>,
    path: &str,
    options: &raven_spawn::SpawnOptions<'_>,
) -> Result<Handle, raven_spawn::SpawnError> {
    let mut sys = NamedSys {
        inner: raven_spawn::RtSys,
        command: command_name(path, options),
    };
    match raven_spawn::spawn_with(initfs, path, options, &mut sys) {
        Err(raven_spawn::SpawnError::NotFound) => {
            // K176: after a root pivot the service executables (login, the shell and
            // their children) live on the mounted root, not in the bootstrap initfs.
            // Fall back to the file server for paths the initfs does not carry.
            #[cfg(feature = "alloc")]
            {
                let image = read_file_image(path)?;
                raven_spawn::spawn_image_with(&image, path, options, &mut sys)
            }
            #[cfg(not(feature = "alloc"))]
            {
                Err(raven_spawn::SpawnError::NotFound)
            }
        }
        other => other,
    }
}

#[cfg(feature = "alloc")]
pub use crate::spawn_attrs::*;

/// The pre-P9a runtime error enum, kept until its module is removed with the next
/// `raven-init` change. New code uses [`SpawnError`].
#[doc(hidden)]
pub type LegacySpawnError = crate::spawn_error::SpawnError;
