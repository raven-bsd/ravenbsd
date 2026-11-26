// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `execve(2)` and the raw `PROC_EXEC` request.
//!
//! The runtime reads the image through the shared `raven-spawn` image reader, packs the
//! arguments and environment into one request, and lets the kernel parse/validate the ELF and
//! build the replacement address space. On success the kernel never returns to this code.

#![allow(unsafe_code)] // the raw syscall and the user-pointer request

use alloc_crate::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::sys::syscall1;

/// Largest image the kernel copy loader accepts.
pub const MAX_EXEC_IMAGE: usize = 1024 * 1024;
/// Raw `proc_exec(request) -> 0` number (the ABI crate is reviewer-owned).
pub const PROC_EXEC: usize = 0x6a;
/// Maximum argv or envp entries.
pub const MAX_ARG_COUNT: usize = 4096;
/// Maximum combined argv/envp string bytes.
pub const MAX_ARG_BYTES: usize = 128 * 1024;
/// Maximum path bytes carried as `AT_EXECFN`.
pub const MAX_PATH: usize = 4096;

/// One `PROC_EXEC` request. The kernel validates every pointer and length.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ExecRequest {
    /// User pointer to the ELF image.
    pub image: u64,
    /// Image length.
    pub image_len: u64,
    /// User pointer to the path bytes.
    pub path: u64,
    /// Path length.
    pub path_len: u64,
    /// User pointer to an array of `argc` string pointers.
    pub argv: u64,
    /// Argument count.
    pub argc: u64,
    /// User pointer to an array of `envc` string pointers.
    pub envp: u64,
    /// Environment count.
    pub envc: u64,
    /// User pointer to an array of `close_count` `u64` handle values.
    pub close_list: u64,
    /// Handles to close before the new image starts.
    pub close_count: u64,
    /// Reserved; must be zero.
    pub flags: u64,
}

const _: () = assert!(core::mem::size_of::<ExecRequest>() == 88);

/// Execute `path` with `argv`/`envp`; the image is read through the `file:` scheme.
///
/// # Errors
/// [`Errno`] from the file reader or [`PROC_EXEC`]; on success this does not return.
#[cfg(all(feature = "alloc", any(target_os = "none", target_os = "ravenbsd")))]
pub fn execve(path: &[u8], argv: &[&[u8]], envp: &[&[u8]], close: &[Handle]) -> Result<(), Errno> {
    let path_text = core::str::from_utf8(path).map_err(|_| Errno::EINVAL)?;
    let image = raven_spawn::read_file_image(path_text).map_err(spawn_errno)?;
    exec_image(&image, path, argv, envp, close)
}

/// Execute an in-memory ELF image.
///
/// # Errors
/// [`Errno`] from the kernel; on success this does not return.
pub fn exec_image(
    image: &[u8],
    path: &[u8],
    argv: &[&[u8]],
    envp: &[&[u8]],
    close: &[Handle],
) -> Result<(), Errno> {
    if image.len() > MAX_EXEC_IMAGE {
        return Err(Errno::EFBIG);
    }
    if path.len() > MAX_PATH {
        return Err(Errno::ENAMETOOLONG);
    }
    if argv.len() > MAX_ARG_COUNT || envp.len() > MAX_ARG_COUNT {
        return Err(Errno::E2BIG);
    }
    let mut strings = Vec::new();
    let mut argv_offsets = Vec::new();
    pack(&mut strings, argv, &mut argv_offsets)?;
    let mut env_offsets = Vec::new();
    pack(&mut strings, envp, &mut env_offsets)?;
    if strings.len() > MAX_ARG_BYTES {
        return Err(Errno::E2BIG);
    }
    let base = strings.as_ptr() as u64;
    let argv_ptrs: Vec<u64> = argv_offsets
        .iter()
        .map(|offset| base.saturating_add(*offset))
        .collect();
    let env_ptrs: Vec<u64> = env_offsets
        .iter()
        .map(|offset| base.saturating_add(*offset))
        .collect();
    let closes: Vec<u64> = close.iter().map(|handle| u64::from(handle.0)).collect();
    let request = ExecRequest {
        image: image.as_ptr() as u64,
        image_len: image.len() as u64,
        path: path.as_ptr() as u64,
        path_len: path.len() as u64,
        argv: argv_ptrs.as_ptr() as u64,
        argc: argv_ptrs.len() as u64,
        envp: env_ptrs.as_ptr() as u64,
        envc: env_ptrs.len() as u64,
        close_list: closes.as_ptr() as u64,
        close_count: closes.len() as u64,
        flags: 0,
    };
    exec_raw(&request)
}

/// Issue one raw `PROC_EXEC` request.
///
/// The kernel validates the pointers and lengths; a malformed request fails with `EFAULT`,
/// `EINVAL`, `E2BIG` or `EFBIG` without touching the calling image.
///
/// # Errors
/// [`Errno`] from the kernel; on success this does not return.
pub fn exec_raw(request: &ExecRequest) -> Result<(), Errno> {
    // SAFETY: the request is a live `#[repr(C)]` value; the kernel reads it through usercopy.
    let result = unsafe { syscall1(PROC_EXEC, request as *const ExecRequest as usize) };
    Errno::from_ret(result).map(|_| ())
}

fn pack(strings: &mut Vec<u8>, list: &[&[u8]], offsets: &mut Vec<u64>) -> Result<(), Errno> {
    offsets.try_reserve(list.len()).map_err(|_| Errno::ENOMEM)?;
    for item in list {
        let offset = u64::try_from(strings.len()).map_err(|_| Errno::E2BIG)?;
        let next = strings
            .len()
            .checked_add(item.len())
            .and_then(|value| value.checked_add(1))
            .ok_or(Errno::E2BIG)?;
        if next > MAX_ARG_BYTES {
            return Err(Errno::E2BIG);
        }
        strings.extend_from_slice(item);
        strings.push(0);
        offsets.push(offset);
    }
    Ok(())
}

#[cfg(all(feature = "alloc", any(target_os = "none", target_os = "ravenbsd")))]
fn spawn_errno(error: raven_spawn::SpawnError) -> Errno {
    use raven_spawn::SpawnError;
    match error {
        SpawnError::Sys(error) => error,
        SpawnError::NotFound | SpawnError::FileNotFound => Errno::ENOENT,
        SpawnError::BadElf(_) | SpawnError::HasInterpreter => Errno::ENOEXEC,
        SpawnError::BadCwd => Errno::EINVAL,
        SpawnError::TooLarge => Errno::EFBIG,
    }
}

#[cfg(test)]
#[path = "exec_ut.rs"]
mod tests;
