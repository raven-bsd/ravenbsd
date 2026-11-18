// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! POSIX shared memory objects over the file-backed `/tmp` namespace.
//!
//! The kernel `MEMOBJ_CREATE` primitive is anonymous and has no name registry, so the SHM option
//! uses a file in the ramfs `/tmp` mount: `name` maps to `/tmp/shm-<name>`. POSIX permits a
//! filesystem-backed implementation and leaves the on-disk visibility unspecified. The shell/ABI
//! export of `shm_open`/`shm_unlink` is a separate ABI decision; this module is the platform seam.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;

use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;

/// The directory and prefix of a shared memory object path.
pub const PREFIX: &[u8] = b"/tmp/shm-";

/// Map a POSIX shared memory name to its `/tmp` path.
///
/// A name must begin with `/` and contain one path component. Embedded slashes, `.` and `..`
/// are rejected with `EINVAL`: POSIX leaves them implementation-defined and a flat namespace
/// keeps the object from escaping the shared-memory prefix.
///
/// # Errors
///
/// [`Errno::EINVAL`] for an empty name, a missing leading slash, an embedded slash, a dot
/// component or an embedded NUL.
pub fn path(name: &[u8]) -> Result<Vec<u8>, Errno> {
    let Some(rest) = name.strip_prefix(b"/") else {
        return Err(Errno::EINVAL);
    };
    if rest.is_empty() || rest.contains(&b'/') || rest == b"." || rest == b".." {
        return Err(Errno::EINVAL);
    }
    if rest.contains(&0) {
        return Err(Errno::EINVAL);
    }
    let mut out = Vec::with_capacity(PREFIX.len().saturating_add(rest.len()));
    out.extend_from_slice(PREFIX);
    out.extend_from_slice(rest);
    Ok(out)
}

/// `shm_open`: create or open a shared memory object.
///
/// The resulting descriptor has `FD_CLOEXEC` set and the file is created with the process
/// umask. `O_WRONLY` is rejected; POSIX allows only `O_RDONLY` and `O_RDWR`.
///
/// # Errors
///
/// [`Errno::EINVAL`] for a bad name or access mode, and the platform's `open` errors.
pub fn shm_open<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    name: &[u8],
    flags: u32,
    mode: u32,
) -> Result<i32, Errno> {
    if flags & u32::try_from(fcntl::O_WRONLY).unwrap_or(u32::MAX) != 0 {
        return Err(Errno::EINVAL);
    }
    let path = path(name)?;
    let cloexec = u32::try_from(fcntl::O_CLOEXEC).unwrap_or(0);
    platform.open(&path, flags | cloexec, mode)
}

/// `shm_unlink`: remove a shared memory object name.
///
/// # Errors
///
/// [`Errno::EINVAL`] for a bad name and the platform's `unlink` errors, including `ENOENT`.
pub fn shm_unlink<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    name: &[u8],
) -> Result<(), Errno> {
    let path = path(name)?;
    platform.unlink(&path)
}

#[cfg(test)]
#[path = "shm_ut.rs"]
mod tests;
