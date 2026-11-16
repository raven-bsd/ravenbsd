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
use raven_abi::syscall::{Syscall, THREAD_CREATE_EX, ThreadCreateEx};
use raven_abi::types::{Handle, Personality, map, prot};

use crate::KernelSys;

/// `PROC_FORK` (S50b) is a raw constant in the kernel, not part of `raven_abi::syscall`.
const PROC_FORK: usize = 0x39;

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

/// Install `base` as the user TLS base of `thread` (`THREAD_SET_TLS`, S42).
pub fn set_tls(thread: Handle, base: u64) -> Result<(), Errno> {
    // SAFETY: THREAD_SET_TLS takes two scalar words: the thread handle returned by THREAD_CREATE
    // (which carries SIGNAL) and a canonical user address. The kernel validates the handle rights
    // and the address, and the Linux thread is blocked in the trap that asked for this.
    let result = unsafe {
        raven_rt::sys::syscall2(
            raven_abi::syscall::THREAD_SET_TLS,
            thread.0 as usize,
            base as usize,
        )
    };
    Errno::from_ret(result).map(|_| ())
}

/// Create a COW child of `source` (`PROC_FORK`); the child has no thread until one is created.
pub fn proc_fork(source: Handle) -> Result<Handle, Errno> {
    // SAFETY: PROC_FORK takes the source process handle and a zero flag word. `source` is the
    // process linuxd loaded, which carries ADMIN; the kernel duplicates its address space and
    // handles and returns the child with all rights.
    let result = unsafe { raven_rt::sys::syscall2(PROC_FORK, source.0 as usize, 0) };
    Errno::from_ret(result).map(|value| Handle(value as u32))
}

/// Create a user thread with the extended payload: TLS base, a local tid output and a
/// clear-on-exit word in the target process.
#[allow(clippy::too_many_arguments)]
pub fn thread_create_ex(
    process: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
    tp: u64,
    tid_out: &mut u32,
    clear_tid: u64,
) -> Result<Handle, Errno> {
    let extended = ThreadCreateEx {
        tp: usize::try_from(tp).map_err(|_| Errno::EINVAL)?,
        tid_out: tid_out as *mut u32 as usize,
        clear_tid_addr: usize::try_from(clear_tid).map_err(|_| Errno::EINVAL)?,
    };
    // SAFETY: THREAD_CREATE takes five scalar words plus a pointer to the live `ThreadCreateEx`
    // below. The kernel validates the target handle, entry/stack VMAs, the TLS address, the local
    // tid output and the target clear-tid word, then copies the payload before returning.
    let result = unsafe {
        raven_rt::sys::syscall6(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack_top,
            arg,
            THREAD_CREATE_EX,
            &extended as *const ThreadCreateEx as usize,
        )
    };
    Errno::from_ret(result).map(|value| Handle(value as u32))
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
