// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The only `unsafe` code in `linuxd`.
//!
//! Three operations have no safe wrapper in `raven-rt` yet and are named as acceptable raw
//! wrappers by the B20 ticket:
//!
//! * `SCHEME_REGISTER` must return the server handle, but `raven_rt::sys::scheme_register`
//!   discards it and the personality handler needs exactly that handle;
//! * `PROC_SET_PERSONALITY` (`0x34`) is a raw constant, not a `Syscall` variant;
//! * reading a Linux process's memory needs a scratch mapping of its memory object, which the
//!   `Sys` surface has no read call for.
//!
//! Every block names its invariant; the kernel validates handles, rights and ranges.

use linuxd::Region;
use linuxd::Sys as _;
use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Personality, map, prot};

use crate::KernelSys;

/// Register `name` and keep the returned scheme-server handle.
pub fn scheme_register(name: &str) -> Result<Handle, Errno> {
    // SAFETY: SCHEME_REGISTER reads `name.len()` bytes from the mapped string literal, takes the
    // scalar self-process handle and returns a new handle; the kernel checks the name and rights.
    let result = unsafe {
        raven_rt::sys::syscall3(
            Syscall::SCHEME_REGISTER as usize,
            name.as_ptr() as usize,
            name.len(),
            Handle::SELF_PROC.0 as usize,
        )
    };
    Errno::from_ret(result).map(|value| Handle(value as u32))
}

/// Mark `process` as Linux and give it `handler` for its traps.
pub fn set_personality(process: Handle, handler: Handle) -> Result<(), Errno> {
    // SAFETY: PROC_SET_PERSONALITY takes three scalar words. `process` was returned by
    // PROC_CREATE (all rights) and has no thread yet, `Personality::Linux` is 1, and `handler` is
    // the scheme-server handle this process just registered; the kernel duplicates it.
    let result = unsafe {
        raven_rt::sys::syscall3(
            raven_abi::syscall::PROC_SET_PERSONALITY,
            process.0 as usize,
            Personality::Linux as usize,
            handler.0 as usize,
        )
    };
    Errno::from_ret(result).map(|_| ())
}

/// Copy `out.len()` child bytes at `address` into `out`.
pub fn copy_in(
    sys: &mut KernelSys,
    regions: &[Region],
    address: u64,
    out: &mut [u8],
) -> Result<(), Errno> {
    let context = map_scratch(sys, regions, address, out.len())?;
    // SAFETY: `map_scratch` mapped the backing object read/write into this process and returned its
    // base plus the in-object offset; the range `offset..offset + out.len()` lies inside the
    // mapping because the region contained the whole request.
    unsafe {
        let source = (context.address + context.offset) as *const u8;
        out.copy_from_slice(core::slice::from_raw_parts(source, out.len()));
    }
    sys.unmap(Handle::SELF_PROC, context.address, context.len)
}

/// Copy `bytes` into the child at `address`.
pub fn copy_out(
    sys: &mut KernelSys,
    regions: &[Region],
    address: u64,
    bytes: &[u8],
) -> Result<(), Errno> {
    let context = map_scratch(sys, regions, address, bytes.len())?;
    // SAFETY: as in `copy_in`, but the mapping is written; `offset + bytes.len()` is inside it.
    unsafe {
        let destination = (context.address + context.offset) as *mut u8;
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
    }
    sys.unmap(Handle::SELF_PROC, context.address, context.len)
}

struct Scratch {
    address: usize,
    offset: usize,
    len: usize,
}

fn map_scratch(
    sys: &mut KernelSys,
    regions: &[Region],
    address: u64,
    len: usize,
) -> Result<Scratch, Errno> {
    let len_u64 = u64::try_from(len).map_err(|_| Errno::EINVAL)?;
    let region = regions
        .iter()
        .find(|region| region.contains(address, len_u64))
        .ok_or(Errno::EFAULT)?;
    if region.backing == Handle::NONE {
        return Err(Errno::EFAULT);
    }
    let end = region
        .backing_offset
        .checked_add(region.len)
        .ok_or(Errno::EINVAL)?;
    let total = end.checked_add(PAGE - 1).ok_or(Errno::EINVAL)? / PAGE * PAGE;
    let total = usize::try_from(total).map_err(|_| Errno::EINVAL)?;
    let flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
    let scratch = sys.map(Handle::SELF_PROC, region.backing, 0, 0, total, flags)?;
    let offset = region.backing_offset + (address - region.start);
    Ok(Scratch {
        address: scratch,
        offset: usize::try_from(offset).map_err(|_| Errno::EINVAL)?,
        len: total,
    })
}

const PAGE: u64 = 4096;
