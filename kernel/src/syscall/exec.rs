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
const ELF_HEADER_BYTES: usize = 64;
const PHDR_BYTES: usize = 56;
const MAX_PHDRS: usize = 64;
/// Header + program headers + `PT_INTERP` only; the image itself stays in user memory.
const MAX_HEADER_BYTES: usize = 64 * 1024;
const PT_INTERP: u32 = 3;

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
    let header = read_header(request.image, image_len)?;

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

    exec::replace_image_from_user(
        &caller,
        &header,
        request.image,
        request.image_len,
        &path,
        &argv,
        &envp,
        &close,
    )?;
    Ok(0)
}

/// Read the ELF header, the complete program-header table and any `PT_INTERP` string.
///
/// The returned buffer is at most [`MAX_HEADER_BYTES`]; the `PT_LOAD` bytes stay in the caller's
/// image and are streamed by `load_segments_from_user`.
fn read_header(image: u64, image_len: usize) -> Result<Vec<u8>, Errno> {
    if image_len < ELF_HEADER_BYTES {
        return Err(Errno::ENOEXEC);
    }
    let mut fixed = [0u8; ELF_HEADER_BYTES];
    copy_from_user(&mut fixed, UserSlice::new(image, ELF_HEADER_BYTES as u64)?)?;
    if !fixed.starts_with(b"\x7fELF") {
        return Err(Errno::ENOEXEC);
    }
    let phoff = header_word(&fixed, 32).ok_or(Errno::ENOEXEC)?;
    let phentsize = header_half(&fixed, 54).ok_or(Errno::ENOEXEC)?;
    let phnum = header_half(&fixed, 56).ok_or(Errno::ENOEXEC)?;
    if usize::from(phentsize) != PHDR_BYTES || phnum == 0 || usize::from(phnum) > MAX_PHDRS {
        return Err(Errno::ENOEXEC);
    }
    let table_len = u64::from(phnum)
        .checked_mul(PHDR_BYTES as u64)
        .ok_or(Errno::ENOEXEC)?;
    let table_end = phoff.checked_add(table_len).ok_or(Errno::ENOEXEC)?;
    if table_end > image_len as u64 {
        return Err(Errno::ENOEXEC);
    }
    if table_end > MAX_HEADER_BYTES as u64 {
        return Err(Errno::EFBIG);
    }
    let mut header = Vec::new();
    let table_end = usize::try_from(table_end).map_err(|_| Errno::EFBIG)?;
    let header_len = table_end.max(ELF_HEADER_BYTES);
    header
        .try_reserve_exact(header_len)
        .map_err(|_| Errno::ENOMEM)?;
    header.resize(header_len, 0);
    header
        .get_mut(..ELF_HEADER_BYTES)
        .ok_or(Errno::ENOEXEC)?
        .copy_from_slice(&fixed);
    let table_len = usize::try_from(table_len).map_err(|_| Errno::EFBIG)?;
    let phoff = usize::try_from(phoff).map_err(|_| Errno::ENOEXEC)?;
    let table = header
        .get_mut(phoff..phoff.saturating_add(table_len))
        .ok_or(Errno::ENOEXEC)?;
    copy_from_user(
        table,
        UserSlice::new(
            image.checked_add(phoff as u64).ok_or(Errno::EFAULT)?,
            table_len as u64,
        )?,
    )?;

    // A static OpenSSH helper has no PT_INTERP; a PIE image names one, and its bytes must be in
    // the header buffer for the parser to return the path.
    let mut index = 0usize;
    while index < usize::from(phnum) {
        let base = phoff
            .checked_add(index.saturating_mul(PHDR_BYTES))
            .ok_or(Errno::ENOEXEC)?;
        let kind = header_u32(&header, base).ok_or(Errno::ENOEXEC)?;
        if kind == PT_INTERP {
            let offset = header_word(&header, base.saturating_add(8)).ok_or(Errno::ENOEXEC)?;
            let len = header_word(&header, base.saturating_add(32)).ok_or(Errno::ENOEXEC)?;
            let end = offset.checked_add(len).ok_or(Errno::ENOEXEC)?;
            if end > image_len as u64 {
                return Err(Errno::ENOEXEC);
            }
            if end > MAX_HEADER_BYTES as u64 {
                return Err(Errno::EFBIG);
            }
            let end = usize::try_from(end).map_err(|_| Errno::EFBIG)?;
            if end > header.len() {
                header
                    .try_reserve(end.saturating_sub(header.len()))
                    .map_err(|_| Errno::ENOMEM)?;
                header.resize(end, 0);
            }
            let start = usize::try_from(offset).map_err(|_| Errno::ENOEXEC)?;
            let length = usize::try_from(len).map_err(|_| Errno::ENOEXEC)?;
            let target = header
                .get_mut(start..start.saturating_add(length))
                .ok_or(Errno::ENOEXEC)?;
            copy_from_user(
                target,
                UserSlice::new(image.checked_add(offset).ok_or(Errno::EFAULT)?, len)?,
            )?;
        }
        index = index.saturating_add(1);
    }
    Ok(header)
}

fn header_half(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn header_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn header_word(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    Some(u64::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
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
