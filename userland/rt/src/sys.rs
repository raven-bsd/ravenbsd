// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw syscall instructions and the typed `raven-abi` wrappers.
#![allow(clippy::missing_safety_doc)] // every raw wrapper shares syscall0's safety contract.

use core::sync::atomic::AtomicU32;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, Timespec, futex, map, prot};
use raven_abi::uname::Uname;

use crate::sync::Mutex;

/// Raw syscall with no arguments.
///
/// # Safety
///
/// The caller must use a valid `raven-abi` syscall number and satisfy its pointer arguments.
pub unsafe fn syscall0(nr: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with one argument.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall1(nr: usize, a0: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with two arguments.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall2(nr: usize, a0: usize, a1: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                in("rsi") a1,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with three arguments.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall3(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                in("rsi") a1,
                in("rdx") a2,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with four arguments.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall4(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                in("rsi") a1,
                in("rdx") a2,
                in("r10") a3,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                in("x3") a3,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                in("a3") a3,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with five arguments.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall5(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                in("rsi") a1,
                in("rdx") a2,
                in("r10") a3,
                in("r8") a4,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                in("x3") a3,
                in("x4") a4,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                in("a3") a3,
                in("a4") a4,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with six arguments.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall6(
    nr: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
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
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                in("x3") a3,
                in("x4") a4,
                in("x5") a5,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                in("a3") a3,
                in("a4") a4,
                in("a5") a5,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

fn checked(value: isize) -> Result<usize, Errno> {
    Errno::from_ret(value)
}

/// Terminate the calling process.
pub fn exit(code: i32) -> ! {
    // SAFETY: EXIT takes only the scalar exit code and never returns.
    unsafe {
        let _ = syscall1(Syscall::EXIT as usize, code as usize);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Write bytes to the debug console.
pub fn debug_write(bytes: &[u8]) -> Result<usize, Errno> {
    // SAFETY: DEBUG_WRITE reads `bytes.len()` bytes from the slice's valid pointer.
    checked(unsafe {
        syscall2(
            Syscall::DEBUG_WRITE as usize,
            bytes.as_ptr() as usize,
            bytes.len(),
        )
    })
}

/// Read bytes typed on the debug console.
///
/// Blocks until at least one byte is available or `timeout_ns` expires (`0` waits forever).
/// Returns the number of bytes stored (at most `buffer.len()`); `Ok(0)` means the timeout
/// expired before any input arrived.
pub fn debug_read(buffer: &mut [u8], timeout_ns: u64) -> Result<usize, Errno> {
    // SAFETY: DEBUG_READ writes at most `buffer.len()` bytes into the slice's valid pointer and
    // the kernel validates that user range before touching it.
    checked(unsafe {
        syscall3(
            Syscall::DEBUG_READ as usize,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            timeout_ns as usize,
        )
    })
}

/// Read the system identification record.
pub fn uname() -> Result<Uname, Errno> {
    let mut uname = Uname::zeroed();
    // SAFETY: UNAME writes one `Uname` into the valid `&mut uname` pointer.
    checked(unsafe { syscall1(Syscall::UNAME as usize, &mut uname as *mut Uname as usize) })?;
    Ok(uname)
}

/// Read a clock (`clock::REALTIME` or `clock::MONOTONIC`).
pub fn clock_get(clock_id: u32) -> Result<Timespec, Errno> {
    let mut timespec = Timespec::default();
    // SAFETY: CLOCK_GET writes one `Timespec` into the valid `&mut timespec` pointer.
    checked(unsafe {
        syscall2(
            Syscall::CLOCK_GET as usize,
            clock_id as usize,
            &mut timespec as *mut Timespec as usize,
        )
    })?;
    Ok(timespec)
}

/// Yield the CPU to another runnable thread.
pub fn yield_now() {
    // SAFETY: YIELD takes no arguments and only yields the CPU.
    unsafe {
        let _ = syscall0(Syscall::YIELD as usize);
    }
}

/// Sleep for at least `ns` nanoseconds.
pub fn nanosleep(ns: u64) -> Result<(), Errno> {
    // SAFETY: NANOSLEEP takes only the scalar nanosecond duration.
    checked(unsafe { syscall1(Syscall::NANOSLEEP as usize, ns as usize) })?;
    Ok(())
}

/// Wait on a futex word.
pub fn futex_wait(
    address: &AtomicU32,
    expected: u32,
    timeout_ns: Option<u64>,
) -> Result<(), Errno> {
    // SAFETY: FUTEX with WAIT reads the `AtomicU32` behind the valid `address` reference.
    checked(unsafe {
        syscall4(
            Syscall::FUTEX as usize,
            address.as_ptr() as usize,
            futex::WAIT as usize,
            expected as usize,
            timeout_ns.unwrap_or(0) as usize,
        )
    })?;
    Ok(())
}

/// Wake up to `count` futex waiters.
pub fn futex_wake(address: &AtomicU32, count: u32) -> Result<usize, Errno> {
    // SAFETY: FUTEX with WAKE reads the `AtomicU32` behind the valid `address` reference.
    checked(unsafe {
        syscall3(
            Syscall::FUTEX as usize,
            address.as_ptr() as usize,
            futex::WAKE as usize,
            count as usize,
        )
    })
}

/// Close a handle.
pub fn handle_close(handle: Handle) -> Result<(), Errno> {
    // SAFETY: HANDLE_CLOSE takes a scalar handle value.
    checked(unsafe { syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize) })?;
    Ok(())
}

/// Duplicate a handle with an intersected rights mask.
pub fn handle_dup(handle: Handle, rights: Rights) -> Result<Handle, Errno> {
    // SAFETY: HANDLE_DUP takes a scalar handle and rights mask.
    let value = checked(unsafe {
        syscall2(
            Syscall::HANDLE_DUP as usize,
            handle.0 as usize,
            rights.bits() as usize,
        )
    })?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn handle_from_ret(value: isize) -> Result<Handle, Errno> {
    let value = Errno::from_ret(value)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Register a scheme name for the calling process.
pub fn scheme_register(name: &str) -> Result<(), Errno> {
    // SAFETY: SCHEME_REGISTER reads `name.len()` bytes and takes the scalar self-process handle.
    checked(unsafe {
        syscall3(
            Syscall::SCHEME_REGISTER as usize,
            name.as_ptr() as usize,
            name.len(),
            Handle::SELF_PROC.0 as usize,
        )
    })
    .map(|_| ())
}

/// Open a scheme path with S11 open flags.
pub fn open(path: &str, flags: u32) -> Result<Handle, Errno> {
    // SAFETY: OPEN reads `path.len()` bytes and takes scalar flags.
    handle_from_ret(unsafe {
        syscall3(
            Syscall::OPEN as usize,
            path.as_ptr() as usize,
            path.len(),
            flags as usize,
        )
    })
}

/// Read from a scheme file.
pub fn read(handle: Handle, buffer: &mut [u8]) -> Result<usize, Errno> {
    // SAFETY: READ writes at most `buffer.len()` bytes into the valid buffer.
    checked(unsafe {
        syscall3(
            Syscall::READ as usize,
            handle.0 as usize,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
        )
    })
}

/// Write to a scheme file.
pub fn write(handle: Handle, buffer: &[u8]) -> Result<usize, Errno> {
    // SAFETY: WRITE reads `buffer.len()` bytes from the valid buffer.
    checked(unsafe {
        syscall3(
            Syscall::WRITE as usize,
            handle.0 as usize,
            buffer.as_ptr() as usize,
            buffer.len(),
        )
    })
}

/// Close a scheme file.
pub fn close(handle: Handle) -> Result<(), Errno> {
    // SAFETY: CLOSE takes a scalar scheme-file handle.
    checked(unsafe { syscall1(Syscall::CLOSE as usize, handle.0 as usize) }).map(|_| ())
}

/// Duplicate a scheme file with a rights mask.
pub fn dup(handle: Handle, rights: u32) -> Result<Handle, Errno> {
    // SAFETY: DUP takes a scalar handle and rights mask.
    handle_from_ret(unsafe { syscall2(Syscall::DUP as usize, handle.0 as usize, rights as usize) })
}

/// Read the 64-byte `FSTAT` record into `out`.
pub fn fstat(handle: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
    // SAFETY: FSTAT writes at most 64 bytes into the valid output record.
    checked(unsafe {
        syscall3(
            Syscall::FSTAT as usize,
            handle.0 as usize,
            out.as_mut_ptr() as usize,
            out.len(),
        )
    })
    .map(|_| ())
}

/// Send a typed `CALL` request with a response buffer.
pub fn call(handle: Handle, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
    // SAFETY: CALL reads the request slice and writes at most `response.len()` bytes.
    checked(unsafe {
        syscall6(
            Syscall::CALL as usize,
            handle.0 as usize,
            request.as_ptr() as usize,
            request.len(),
            response.as_mut_ptr() as usize,
            response.len(),
            0,
        )
    })
}

/// Receive one request packet and return the server-side request handle.
pub fn recv(packet: &mut [u8]) -> Result<Handle, Errno> {
    // SAFETY: RECV writes one packet into the valid packet buffer.
    handle_from_ret(unsafe {
        syscall2(
            Syscall::RECV as usize,
            packet.as_mut_ptr() as usize,
            packet.len(),
        )
    })
}

/// Send one response packet for a server-side request handle.
pub fn send(request: Handle, packet: &[u8]) -> Result<(), Errno> {
    // SAFETY: SEND reads one encoded packet from the valid slice.
    checked(unsafe {
        syscall3(
            Syscall::SEND as usize,
            request.0 as usize,
            packet.as_ptr() as usize,
            packet.len(),
        )
    })
    .map(|_| ())
}

/// Create a thread in `process` at `entry` with `stack_top` and one argument.
pub fn thread_create(
    process: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    // SAFETY: THREAD_CREATE takes scalar addresses; the kernel validates the entry and stack VMAs.
    handle_from_ret(unsafe {
        syscall5(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack_top,
            arg,
            0,
        )
    })
}

/// Terminate the calling thread (and process if it is the last thread).
pub fn thread_exit(code: i32) -> ! {
    // SAFETY: THREAD_EXIT takes only the scalar exit code and never returns.
    unsafe {
        let _ = syscall1(Syscall::THREAD_EXIT as usize, code as usize);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Map anonymous read/write memory and return its base address.
pub fn map_anonymous(len: usize) -> Result<usize, Errno> {
    // SAFETY: MAP takes scalar arguments only; the kernel validates the length and flags.
    checked(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            0,
            len,
            map::pack(prot::READ | prot::WRITE, 0),
        )
    })
}

/// Unmap a range in `process`.
pub fn unmap(process: Handle, address: usize, len: usize) -> Result<(), Errno> {
    // SAFETY: UNMAP takes scalar arguments only; the kernel validates the range.
    checked(unsafe { syscall3(Syscall::UNMAP as usize, process.0 as usize, address, len) })
        .map(|_| ())
}

/// Fixed user window where [`map_initfs`] maps the boot archive.
///
/// The value is the one the S11 fixture uses: above the normal program load address and below
/// the stack at the top of the user range.
const INITFS_WINDOW: usize = 0x1000_0000;
/// `MAP`/`UNMAP` granularity.
const PAGE_SIZE: usize = 4096;
/// End of the canonical user window; ABI v1 uses the same split on every architecture.
const USER_END: usize = 0x0000_7FFF_FFFF_F000;
/// Offset of the little-endian C7 `total_size` field in the initfs header.
const INITFS_TOTAL_SIZE_OFFSET: usize = 40;
/// Length of the C7 archive magic.
const INITFS_MAGIC: &[u8] = b"RAVENFS1";

const _: () = {
    assert!(INITFS_WINDOW.is_multiple_of(PAGE_SIZE));
    assert!(INITFS_WINDOW > PAGE_SIZE);
    assert!(INITFS_WINDOW < USER_END);
};

/// Cached `(address, length)` of the initfs mapping; `None` before the first call.
static INITFS_IMAGE: Mutex<Option<(usize, usize)>> = Mutex::new(None);

/// Map the boot initfs (`Handle::INITFS`) read-only into the calling process.
///
/// The archive is mapped at the fixed [`INITFS_WINDOW`] address for the process lifetime; repeated
/// calls return the same slice. The first page is checked for the C7 magic and a page-aligned
/// `total_size` that fits the user window. The slice is not a validated archive: pass it to
/// `raven_initfs::Initfs::parse` before reading entries.
pub fn map_initfs() -> Result<&'static [u8], Errno> {
    let mut cached = INITFS_IMAGE.lock();
    if let Some((address, length)) = *cached {
        // SAFETY: `address` is the fixed initfs window mapped read-only for `length` bytes by a
        // previous successful call; that mapping is never unmapped and outlives this slice.
        return Ok(unsafe { core::slice::from_raw_parts(address as *const u8, length) });
    }

    let probe = map_initfs_window(PAGE_SIZE)?;
    let total = {
        // SAFETY: `probe` is a one-page read-only initfs mapping created immediately above.
        let header = unsafe { core::slice::from_raw_parts(probe as *const u8, PAGE_SIZE) };
        match initfs_total_size(header) {
            Ok(total) => total,
            Err(error) => {
                let _ = unmap(Handle::SELF_PROC, probe, PAGE_SIZE);
                return Err(error);
            }
        }
    };
    unmap(Handle::SELF_PROC, probe, PAGE_SIZE)?;
    let image = map_initfs_window(total)?;
    *cached = Some((image, total));
    // SAFETY: `image` is the fixed initfs window mapped read-only for `total` bytes; the mapping
    // is never unmapped and stays valid for the process lifetime.
    Ok(unsafe { core::slice::from_raw_parts(image as *const u8, total) })
}

/// Map `len` bytes of the initfs at [`INITFS_WINDOW`] and return the base address.
fn map_initfs_window(len: usize) -> Result<usize, Errno> {
    // SAFETY: MAP takes scalar arguments only; the kernel checks the handle rights, the fixed
    // user range and the fact that the window is still free.
    checked(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::INITFS.0 as usize,
            0,
            INITFS_WINDOW,
            len,
            map::pack(prot::READ, map::FIXED),
        )
    })
}

/// Read and validate the C7 `total_size` from a mapped first page.
fn initfs_total_size(header: &[u8]) -> Result<usize, Errno> {
    let magic = header.get(..INITFS_MAGIC.len()).ok_or(Errno::EINVAL)?;
    if magic != INITFS_MAGIC {
        return Err(Errno::EINVAL);
    }
    let field = header
        .get(INITFS_TOTAL_SIZE_OFFSET..INITFS_TOTAL_SIZE_OFFSET + 8)
        .ok_or(Errno::EINVAL)?;
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(field);
    validate_initfs_size(u64::from_le_bytes(bytes))
}

/// Validate a C7 `total_size` against the page granularity and the fixed window.
fn validate_initfs_size(total: u64) -> Result<usize, Errno> {
    if total == 0 || !total.is_multiple_of(PAGE_SIZE as u64) {
        return Err(Errno::EINVAL);
    }
    let total = usize::try_from(total).map_err(|_| Errno::ENOMEM)?;
    let end = INITFS_WINDOW.checked_add(total).ok_or(Errno::ENOMEM)?;
    if end > USER_END {
        return Err(Errno::ENOMEM);
    }
    Ok(total)
}

/// Install a duplicate of `source` in `dest` with `rights`, returning the destination's handle.
///
/// This is the safe form of C14 `HANDLE_TRANSFER`: rights never grow (`new = source AND rights`),
/// and the destination process must have `WRITE`. `Handle::NONE` and an empty rights mask are
/// rejected before the syscall. Transferring to the calling process is `HANDLE_DUP`.
pub fn handle_transfer(dest: Handle, source: Handle, rights: Rights) -> Result<Handle, Errno> {
    validate_transfer(dest, source, rights)?;
    // SAFETY: HANDLE_TRANSFER takes scalar arguments only; the kernel validates both handles, the
    // destination's WRITE right and the source's TRANSFER right, and never grows rights.
    handle_from_ret(unsafe {
        syscall3(
            Syscall::HANDLE_TRANSFER as usize,
            dest.0 as usize,
            source.0 as usize,
            rights.0 as usize,
        )
    })
}

/// Reject the arguments C14 defines as invalid before entering the kernel.
fn validate_transfer(dest: Handle, source: Handle, rights: Rights) -> Result<(), Errno> {
    if dest == Handle::NONE || source == Handle::NONE || rights == Rights::NONE {
        return Err(Errno::EINVAL);
    }
    Ok(())
}

#[cfg(test)]
#[path = "sys_ut.rs"]
mod tests;
