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
use raven_abi::types::{Handle, Personality, Rights, map, prot};

use crate::KernelSys;

/// Raw `PREAD` (PG-22): `(handle, buffer, len, offset) -> bytes`.
const PREAD: usize = 0x6b;
/// Raw `PWRITE` (PG-22): `(handle, buffer, len, offset) -> bytes`.
const PWRITE: usize = 0x6c;
/// Raw `SEEK` (PG-22): `(handle, offset, whence) -> new offset`.
const SEEK: usize = 0x6d;

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

/// Open a `file:`/`ufs:` scheme path with Linux `open` flags.
///
/// `raven_rt::fs::s11_flags` maps the Linux flag word to the S11 wire flags the scheme servers
/// expect; the kernel performs the server `OPEN` and returns the scheme-file handle.
pub fn open_file(path: &str, flags: i32) -> Result<Handle, Errno> {
    // The S11 open flags cover access, create/truncate/append, directory and no-follow. The
    // descriptor/status bits are handled by linuxd's own descriptor table.
    let flags = flags
        & !(linuxd::files::O_CLOEXEC
            | linuxd::files::O_NONBLOCK
            | linuxd::files::O_LARGEFILE
            | linuxd::files::O_NOCTTY);
    let wire = raven_rt::fs::s11_flags(flags as u32)?;
    raven_rt::sys::open(path, wire)
}

/// Read from a scheme file at its cursor.
pub fn read_file(handle: Handle, buffer: &mut [u8]) -> Result<usize, Errno> {
    raven_rt::sys::read(handle, buffer)
}

/// Write to a scheme file at its cursor.
pub fn write_file(handle: Handle, buffer: &[u8]) -> Result<usize, Errno> {
    raven_rt::sys::write(handle, buffer)
}

/// Close a scheme file.
pub fn close_file(handle: Handle) -> Result<(), Errno> {
    raven_rt::sys::close(handle)
}

/// Duplicate a scheme file with all rights; the kernel intersects them with the original.
pub fn dup_file(handle: Handle) -> Result<Handle, Errno> {
    raven_rt::sys::dup(handle, Rights::ALL.bits())
}

/// Positional read; the scheme cursor does not move.
pub fn pread_file(handle: Handle, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
    // SAFETY: PREAD takes the scheme handle, a writable buffer of `buffer.len()` bytes and the
    // scalar offset. The kernel validates the handle rights and copies at most `len` bytes into
    // the buffer.
    let result = unsafe {
        raven_rt::sys::syscall4(
            PREAD,
            handle.0 as usize,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            offset as usize,
        )
    };
    Errno::from_ret(result)
}

/// Positional write; the scheme cursor does not move.
pub fn pwrite_file(handle: Handle, offset: u64, buffer: &[u8]) -> Result<usize, Errno> {
    // SAFETY: PWRITE takes the scheme handle, a readable buffer of `buffer.len()` bytes and the
    // scalar offset; the kernel validates the handle rights and copies at most `len` bytes from
    // the buffer.
    let result = unsafe {
        raven_rt::sys::syscall4(
            PWRITE,
            handle.0 as usize,
            buffer.as_ptr() as usize,
            buffer.len(),
            offset as usize,
        )
    };
    Errno::from_ret(result)
}

/// Move a scheme file's cursor and return the new offset.
pub fn seek_file(handle: Handle, offset: i64, whence: i32) -> Result<u64, Errno> {
    // SAFETY: SEEK takes the scheme handle and two scalar words; the kernel validates the handle
    // and the whence value.
    let result = unsafe {
        raven_rt::sys::syscall3(
            SEEK,
            handle.0 as usize,
            offset as usize,
            usize::try_from(whence).map_err(|_| Errno::EINVAL)?,
        )
    };
    Errno::from_ret(result).map(|value| value as u64)
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
