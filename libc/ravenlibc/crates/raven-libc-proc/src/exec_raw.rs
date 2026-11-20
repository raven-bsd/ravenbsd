// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Native `PROC_EXEC` (`0x6a`) for the libc `execve`/`execv` path.
//!
//! The image is read through the shared `raven-spawn` reader (bounded at
//! [`raven_spawn::MAX_FILE_IMAGE`]), then the kernel is given the user image pointer and the
//! path/argv/envp/CLOEXEC request. The kernel copies only the ELF headers and streams the
//! `PT_LOAD` segments; this module never builds a kernel-side image copy.

use alloc::vec::Vec;

use raven_abi::types::Handle;

use crate::errors::{E2BIG, EFBIG, EINVAL, ENAMETOOLONG, ENOENT, ENOEXEC, Errno};
use crate::request::ExecRequest;

/// Raw `proc_exec(request) -> 0` number (the ABI crate is reviewer-owned).
pub const PROC_EXEC: usize = 0x6a;
/// Maximum argv or envp entries, matching the kernel.
const MAX_ARG_COUNT: usize = 4096;
/// Maximum combined argv/envp string bytes, matching the kernel.
const MAX_ARG_BYTES: usize = 128 * 1024;
/// Maximum path bytes, matching the kernel.
const MAX_PATH: usize = 4096;

/// The 88-byte flat request consumed by `PROC_EXEC`, in field order.
const REQUEST_WORDS: usize = 11;
type KernelRequest = [u8; REQUEST_WORDS * 8];

fn put_word(request: &mut KernelRequest, index: usize, value: u64) {
    let offset = index.saturating_mul(8);
    if let Some(slot) = request.get_mut(offset..offset.saturating_add(8)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

/// Execute one backend [`ExecRequest`] through `PROC_EXEC`.
///
/// On success the kernel replaces the caller and this function does not return. A failure is a
/// positive `errno` and leaves the caller's image untouched.
pub fn exec(request: &ExecRequest) -> Errno {
    let image = match read_image(request) {
        Ok(image) => image,
        Err(error) => return error,
    };
    if image.len() > raven_spawn::MAX_FILE_IMAGE {
        return EFBIG;
    }
    if request.path.len() > MAX_PATH {
        return ENAMETOOLONG;
    }
    if request.argv.len() > MAX_ARG_COUNT || request.envp.len() > MAX_ARG_COUNT {
        return E2BIG;
    }
    let mut strings = Vec::new();
    let mut argv_offsets = Vec::new();
    if let Err(error) = pack(&mut strings, &request.argv, &mut argv_offsets) {
        return error;
    }
    let mut env_offsets = Vec::new();
    if let Err(error) = pack(&mut strings, &request.envp, &mut env_offsets) {
        return error;
    }
    if strings.len() > MAX_ARG_BYTES {
        return E2BIG;
    }
    let base = strings.as_ptr() as u64;
    let argv: Vec<u64> = argv_offsets
        .iter()
        .map(|offset| base.saturating_add(*offset))
        .collect();
    let envp: Vec<u64> = env_offsets
        .iter()
        .map(|offset| base.saturating_add(*offset))
        .collect();
    let close: Vec<u64> = request
        .close_list
        .iter()
        .map(|handle| u64::from(*handle))
        .collect();
    let mut kernel: KernelRequest = [0u8; REQUEST_WORDS * 8];
    put_word(&mut kernel, 0, image.as_ptr() as u64);
    put_word(&mut kernel, 1, image.len() as u64);
    put_word(&mut kernel, 2, request.path.as_ptr() as u64);
    put_word(&mut kernel, 3, request.path.len() as u64);
    put_word(&mut kernel, 4, argv.as_ptr() as u64);
    put_word(&mut kernel, 5, argv.len() as u64);
    put_word(&mut kernel, 6, envp.as_ptr() as u64);
    put_word(&mut kernel, 7, envp.len() as u64);
    put_word(&mut kernel, 8, close.as_ptr() as u64);
    put_word(&mut kernel, 9, close.len() as u64);
    put_word(&mut kernel, 10, 0);
    match raven_libc_sys::sys::proc_exec(&kernel) {
        Ok(_) => 0,
        Err(error) => i32::from(error.0),
    }
}

fn read_image(request: &ExecRequest) -> Result<Vec<u8>, Errno> {
    if request.fd_handle != u32::MAX {
        return raven_spawn::read_file_handle(Handle(request.fd_handle)).map_err(spawn_errno);
    }
    let path = core::str::from_utf8(&request.path).map_err(|_| EINVAL)?;
    if path.is_empty() {
        return Err(EINVAL);
    }
    raven_spawn::read_file_image(path).map_err(spawn_errno)
}

fn spawn_errno(error: raven_spawn::SpawnError) -> Errno {
    use raven_spawn::SpawnError;
    match error {
        SpawnError::Sys(error) => i32::from(error.0),
        SpawnError::NotFound | SpawnError::FileNotFound => ENOENT,
        SpawnError::BadElf(_) | SpawnError::HasInterpreter => ENOEXEC,
        SpawnError::BadCwd => EINVAL,
        SpawnError::TooLarge => EFBIG,
    }
}

fn pack(strings: &mut Vec<u8>, list: &[Vec<u8>], offsets: &mut Vec<u64>) -> Result<(), Errno> {
    offsets
        .try_reserve(list.len())
        .map_err(|_| crate::errors::ENOMEM)?;
    for item in list {
        let offset = u64::try_from(strings.len()).map_err(|_| E2BIG)?;
        let next = strings
            .len()
            .checked_add(item.len())
            .and_then(|value| value.checked_add(1))
            .ok_or(E2BIG)?;
        if next > MAX_ARG_BYTES {
            return Err(E2BIG);
        }
        strings.extend_from_slice(item);
        strings.push(0);
        offsets.push(offset);
    }
    Ok(())
}
