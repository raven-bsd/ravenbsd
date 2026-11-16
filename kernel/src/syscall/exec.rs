// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Native `PROC_EXEC` (`0x6a`): same-pid `execve`.
//!
//! The request is a flat structure because six syscall registers cannot carry the image, path,
//! two pointer arrays and the CLOEXEC handle list. Every pointer is copied through the
//! fault-safe user-copy layer; the kernel parses the image, builds the replacement address space
//! and only then switches. The runtime wrapper lives in `userland/rt/src/exec.rs`.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_usercopy::UserSlice;

use crate::process::exec::{self, MAX_EXEC_IMAGE};
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_from_user;

/// Raw `proc_exec(request) -> 0` number. The ABI crate is reviewer-owned.
pub const PROC_EXEC: usize = 0x6a;

const REQUEST_BYTES: usize = 88;
const MAX_PATH: usize = 4096;
const MAX_COUNT: usize = 4096;
const MAX_STRING: usize = 64 * 1024;
const MAX_ARG_BYTES: usize = 128 * 1024;
const MAX_HANDLES: usize = 65536;

#[derive(Clone, Copy)]
struct Request {
    image: u64,
    image_len: u64,
    path: u64,
    path_len: u64,
    argv: u64,
    argc: u64,
    envp: u64,
    envc: u64,
    close_list: u64,
    close_count: u64,
    flags: u64,
}

impl Request {
    fn parse(pointer: u64) -> Result<Self, Errno> {
        let mut bytes = [0u8; REQUEST_BYTES];
        copy_from_user(&mut bytes, UserSlice::new(pointer, REQUEST_BYTES as u64)?)?;
        Ok(Self {
            image: word(&bytes, 0),
            image_len: word(&bytes, 8),
            path: word(&bytes, 16),
            path_len: word(&bytes, 24),
            argv: word(&bytes, 32),
            argc: word(&bytes, 40),
            envp: word(&bytes, 48),
            envc: word(&bytes, 56),
            close_list: word(&bytes, 64),
            close_count: word(&bytes, 72),
            flags: word(&bytes, 80),
        })
    }
}

/// `proc_exec(request)`.
pub fn exec(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = crate::process::current().ok_or(Errno::ESRCH)?;
    let request = Request::parse(frame.rdi)?;
    if request.flags != 0 {
        return Err(Errno::EINVAL);
    }
    if request.image_len == 0 {
        return Err(Errno::EINVAL);
    }
    if request.image_len > MAX_EXEC_IMAGE as u64 {
        return Err(Errno::EFBIG);
    }
    if request.path_len > MAX_PATH as u64 {
        return Err(Errno::ENAMETOOLONG);
    }
    if request.argc > MAX_COUNT as u64 || request.envc > MAX_COUNT as u64 {
        return Err(Errno::E2BIG);
    }
    if request.close_count > MAX_HANDLES as u64 {
        return Err(Errno::EINVAL);
    }

    let image_len = usize::try_from(request.image_len).map_err(|_| Errno::EFBIG)?;
    let mut image = Vec::new();
    image
        .try_reserve_exact(image_len)
        .map_err(|_| Errno::ENOMEM)?;
    image.resize(image_len, 0);
    copy_from_user(
        &mut image,
        UserSlice::new(request.image, request.image_len)?,
    )?;

    let path_len = usize::try_from(request.path_len).map_err(|_| Errno::ENAMETOOLONG)?;
    let mut path = Vec::new();
    if path_len != 0 {
        path.try_reserve_exact(path_len)
            .map_err(|_| Errno::ENOMEM)?;
        path.resize(path_len, 0);
        copy_from_user(&mut path, UserSlice::new(request.path, request.path_len)?)?;
    }

    let mut total = 0usize;
    let argv = string_array(request.argv, request.argc, &mut total)?;
    let envp = string_array(request.envp, request.envc, &mut total)?;
    let close = close_list(request.close_list, request.close_count)?;

    exec::replace_image(&caller, &image, &path, &argv, &envp, &close)?;
    Ok(0)
}

fn word(bytes: &[u8; REQUEST_BYTES], offset: usize) -> u64 {
    let mut value = [0u8; 8];
    if let Some(slice) = bytes.get(offset..offset.saturating_add(8)) {
        value.copy_from_slice(slice);
    }
    u64::from_le_bytes(value)
}

fn raw_word(bytes: &[u8], offset: usize) -> Result<u64, Errno> {
    let end = offset.checked_add(8).ok_or(Errno::E2BIG)?;
    let slice = bytes.get(offset..end).ok_or(Errno::E2BIG)?;
    let mut value = [0u8; 8];
    value.copy_from_slice(slice);
    Ok(u64::from_le_bytes(value))
}

fn string_array(pointer: u64, count: u64, total: &mut usize) -> Result<Vec<Vec<u8>>, Errno> {
    let count = usize::try_from(count).map_err(|_| Errno::E2BIG)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let bytes_len = count.checked_mul(8).ok_or(Errno::E2BIG)?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(bytes_len)
        .map_err(|_| Errno::ENOMEM)?;
    raw.resize(bytes_len, 0);
    copy_from_user(&mut raw, UserSlice::new(pointer, bytes_len as u64)?)?;
    let mut result = Vec::new();
    result.try_reserve(count).map_err(|_| Errno::ENOMEM)?;
    for index in 0..count {
        let offset = index.checked_mul(8).ok_or(Errno::E2BIG)?;
        let string = copy_cstring(raw_word(&raw, offset)?)?;
        *total = total
            .checked_add(string.len().saturating_add(1))
            .ok_or(Errno::E2BIG)?;
        if *total > MAX_ARG_BYTES {
            return Err(Errno::E2BIG);
        }
        result.push(string);
    }
    Ok(result)
}

fn copy_cstring(pointer: u64) -> Result<Vec<u8>, Errno> {
    let mut bytes = Vec::new();
    loop {
        if bytes.len() >= MAX_STRING {
            return Err(Errno::E2BIG);
        }
        let offset = u64::try_from(bytes.len()).map_err(|_| Errno::E2BIG)?;
        let address = pointer.checked_add(offset).ok_or(Errno::EFAULT)?;
        let mut byte = [0u8; 1];
        copy_from_user(&mut byte, UserSlice::new(address, 1)?)?;
        if byte[0] == 0 {
            return Ok(bytes);
        }
        bytes.push(byte[0]);
    }
}

fn close_list(pointer: u64, count: u64) -> Result<Vec<Handle>, Errno> {
    let count = usize::try_from(count).map_err(|_| Errno::EINVAL)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let bytes_len = count.checked_mul(8).ok_or(Errno::EINVAL)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(bytes_len)
        .map_err(|_| Errno::ENOMEM)?;
    bytes.resize(bytes_len, 0);
    copy_from_user(&mut bytes, UserSlice::new(pointer, bytes_len as u64)?)?;
    let mut handles = Vec::new();
    handles.try_reserve(count).map_err(|_| Errno::ENOMEM)?;
    for index in 0..count {
        let offset = index.checked_mul(8).ok_or(Errno::EINVAL)?;
        let word = raw_word(&bytes, offset)?;
        let value = u32::try_from(word).map_err(|_| Errno::EINVAL)?;
        handles.push(Handle(value));
    }
    Ok(handles)
}
