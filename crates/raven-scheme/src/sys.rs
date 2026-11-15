// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw S11 syscall wrappers used by [`crate::RtKernel`].
//!
//! The register convention is the RavenBSD syscall ABI: number in `rax` (x86-64), `x8`
//! (aarch64) or `a7` (riscv64); arguments in the usual C argument registers. x86-64 declares the
//! full C clobber set because the kernel return path does not preserve scratch registers.

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};

/// Raw `pread(handle, buffer, len, offset)` number (PG-22; the ABI crate owns the enum value).
const PREAD: usize = 0x6b;
/// Raw `pwrite(handle, buffer, len, offset)` number.
const PWRITE: usize = 0x6c;
/// Raw `seek(handle, offset, whence) -> new_offset` number.
const SEEK: usize = 0x6d;

/// One raw syscall with up to six arguments.
#[inline(always)]
pub(crate) fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let result: isize;
        // SAFETY: the caller supplies a valid syscall number and arguments; the kernel validates
        // every pointer, handle and length.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") number => result,
                in("rdi") a0,
                in("rsi") a1,
                in("rdx") a2,
                in("r10") a3,
                in("r8") a4,
                in("r9") a5,
                clobber_abi("C"),
                options(nostack),
            );
        }
        result
    }
    #[cfg(target_arch = "aarch64")]
    {
        let result: usize;
        // SAFETY: the caller supplies a valid syscall number and arguments; the kernel validates
        // every pointer, handle and length.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") number,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                in("x3") a3,
                in("x4") a4,
                in("x5") a5,
                lateout("x0") result,
                options(nostack),
            );
        }
        result as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let result: usize;
        // SAFETY: the caller supplies a valid syscall number and arguments; the kernel validates
        // every pointer, handle and length.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") number,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                in("a3") a3,
                in("a4") a4,
                in("a5") a5,
                lateout("a0") result,
                options(nostack),
            );
        }
        result as isize
    }
}

fn checked(value: isize) -> Result<usize, Errno> {
    Errno::from_ret(value)
}

pub(crate) fn scheme_register(name: &str) -> Result<(), Errno> {
    checked(syscall6(
        Syscall::SCHEME_REGISTER as usize,
        name.as_ptr() as usize,
        name.len(),
        Handle::SELF_PROC.0 as usize,
        0,
        0,
        0,
    ))
    .map(|_| ())
}

pub(crate) fn open(path: &str, flags: u32) -> Result<Handle, Errno> {
    let value = checked(syscall6(
        Syscall::OPEN as usize,
        path.as_ptr() as usize,
        path.len(),
        flags as usize,
        0,
        0,
        0,
    ))?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

pub(crate) fn read(handle: Handle, buffer: &mut [u8]) -> Result<usize, Errno> {
    checked(syscall6(
        Syscall::READ as usize,
        handle.0 as usize,
        buffer.as_mut_ptr() as usize,
        buffer.len(),
        0,
        0,
        0,
    ))
}

pub(crate) fn write(handle: Handle, buffer: &[u8]) -> Result<usize, Errno> {
    checked(syscall6(
        Syscall::WRITE as usize,
        handle.0 as usize,
        buffer.as_ptr() as usize,
        buffer.len(),
        0,
        0,
        0,
    ))
}

pub(crate) fn pread(handle: Handle, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
    checked(syscall6(
        PREAD,
        handle.0 as usize,
        buffer.as_mut_ptr() as usize,
        buffer.len(),
        offset as usize,
        0,
        0,
    ))
}

pub(crate) fn pwrite(handle: Handle, offset: u64, buffer: &[u8]) -> Result<usize, Errno> {
    checked(syscall6(
        PWRITE,
        handle.0 as usize,
        buffer.as_ptr() as usize,
        buffer.len(),
        offset as usize,
        0,
        0,
    ))
}

pub(crate) fn seek(handle: Handle, offset: i64, whence: u32) -> Result<u64, Errno> {
    let result = checked(syscall6(
        SEEK,
        handle.0 as usize,
        offset as isize as usize,
        whence as usize,
        0,
        0,
        0,
    ))?;
    u64::try_from(result).map_err(|_| Errno::EOVERFLOW)
}

pub(crate) fn close(handle: Handle) -> Result<(), Errno> {
    checked(syscall6(
        Syscall::CLOSE as usize,
        handle.0 as usize,
        0,
        0,
        0,
        0,
        0,
    ))
    .map(|_| ())
}

pub(crate) fn dup(handle: Handle, rights: u32) -> Result<Handle, Errno> {
    let value = checked(syscall6(
        Syscall::DUP as usize,
        handle.0 as usize,
        rights as usize,
        0,
        0,
        0,
        0,
    ))?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Install a reduced duplicate of `source` in the scheme server that owns `file`.
pub(crate) fn handle_transfer_file(
    file: Handle,
    source: Handle,
    rights: Rights,
) -> Result<Handle, Errno> {
    let value = checked(syscall6(
        raven_abi::syscall::HANDLE_TRANSFER_FILE,
        file.0 as usize,
        source.0 as usize,
        rights.bits() as usize,
        0,
        0,
        0,
    ))?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

pub(crate) fn fstat(handle: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
    checked(syscall6(
        Syscall::FSTAT as usize,
        handle.0 as usize,
        out.as_mut_ptr() as usize,
        out.len(),
        0,
        0,
        0,
    ))
    .map(|_| ())
}

pub(crate) fn call(handle: Handle, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
    checked(syscall6(
        Syscall::CALL as usize,
        handle.0 as usize,
        request.as_ptr() as usize,
        request.len(),
        response.as_mut_ptr() as usize,
        response.len(),
        0,
    ))
}

pub(crate) fn recv(packet: &mut [u8]) -> Result<Handle, Errno> {
    let value = checked(syscall6(
        Syscall::RECV as usize,
        packet.as_mut_ptr() as usize,
        packet.len(),
        0,
        0,
        0,
        0,
    ))?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

pub(crate) fn send(request: Handle, packet: &[u8]) -> Result<(), Errno> {
    checked(syscall6(
        Syscall::SEND as usize,
        request.0 as usize,
        packet.as_ptr() as usize,
        packet.len(),
        0,
        0,
        0,
    ))
    .map(|_| ())
}
