// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared helpers for the K71 reply-attachment fixture.

#![no_std]
#![allow(unsafe_code)] // Raw syscalls and mapped test objects; every block has a SAFETY comment.

extern crate alloc;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::sys::{syscall1, syscall2, syscall3, syscall6};

/// Page size of the fixture memory objects.
pub const PAGE: usize = 4096;

/// One raw syscall result as a `Result`.
pub fn result(value: isize) -> Result<usize, Errno> {
    Errno::from_ret(value)
}

/// Create one page-sized memory object.
pub fn memobj_create() -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes scalar length and flags; the kernel validates both.
    let value = result(unsafe { syscall2(Syscall::MEMOBJ_CREATE as usize, PAGE, 0) })?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Map `object` into this process.
pub fn map_object(object: Handle, flags: usize) -> Result<usize, Errno> {
    // SAFETY: MAP follows the stable syscall ABI; the kernel validates every argument.
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            object.0 as usize,
            0,
            0,
            PAGE,
            flags,
        )
    })
}

/// Unmap one page previously mapped by [`map_object`].
pub fn unmap(address: usize) {
    // SAFETY: UNMAP takes scalar arguments; the caller owns the one-page mapping.
    let _ = unsafe {
        syscall3(
            Syscall::UNMAP as usize,
            Handle::SELF_PROC.0 as usize,
            address,
            PAGE,
        )
    };
}

/// Close a local handle.
pub fn close(handle: Handle) {
    // SAFETY: HANDLE_CLOSE takes one scalar handle validated by the kernel.
    let _ = unsafe { syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize) };
}

/// Create a memory object whose first byte is `pattern`.
pub fn patterned_object(pattern: u8) -> Result<Handle, Errno> {
    let object = memobj_create()?;
    let address = map_object(object, map::pack(prot::READ | prot::WRITE, map::SHARED))?;
    // SAFETY: the private mapping is one writable page of this object.
    unsafe { (address as *mut u8).write(pattern) };
    unmap(address);
    Ok(object)
}

/// Read the first byte of a mapped object.
pub fn first_byte(object: Handle, expected: u8) -> Result<(), Errno> {
    let address = map_object(object, map::pack(prot::READ, map::SHARED))?;
    // SAFETY: the mapping is one readable page of this object.
    let value = unsafe { (address as *const u8).read() };
    unmap(address);
    if value == expected {
        Ok(())
    } else {
        Err(Errno::EIO)
    }
}

/// Extract one little-endian `(handle, rights_mask)` pair.
pub fn pair_at(buffer: &[u8], offset: usize) -> Option<(Handle, u32)> {
    let bytes = buffer.get(offset..offset.saturating_add(8))?;
    let handle = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let mask = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    Some((Handle(handle), mask))
}
