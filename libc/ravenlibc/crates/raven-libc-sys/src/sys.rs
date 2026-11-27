// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One safe typed wrapper per variant of [`raven_abi::syscall::Syscall`].
//!
//! Signatures use `&[u8]` / `&mut [u8]`, [`Handle`] and [`Errno`]. Handle-returning calls
//! convert with `u32::try_from` and map failure to `EINVAL`; `MAP` decodes its address with
//! [`Errno::from_ret`], never with `ret < 0` (a high user address is a negative `isize`).
//! The wrapper list is covered by [`wrapper_name`] and checked against `Syscall::ALL`.

use core::sync::atomic::AtomicU32;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::fcntl::RecordLock;
use raven_abi::types::{Handle, Rights, Timespec, futex, map as map_flags, prot};
use raven_abi::uname::Uname;

use crate::arch;

/// Number of typed wrappers in this module; must equal `Syscall::ALL.len()`.
pub const WRAPPER_COUNT: usize = 47;

/// Raw `PROC_CHROOT` (`0x0D`): set the calling process's root from a bounded path.
pub fn proc_chroot(path: &[u8]) -> Result<(), Errno> {
    // SAFETY: `path` is a live byte slice; the kernel copies it through the fault-safe user
    // layer and validates the length before resolving it.
    let raw = unsafe {
        arch::syscall2(
            raven_abi::syscall::PROC_CHROOT,
            path.as_ptr() as usize,
            path.len(),
        )
    };
    crate::errno::ret(raw).map(|_| ())
}

/// Raw `GETRANDOM` (`0x08`): fill `buf` from the boot-seeded kernel CSPRNG.
///
/// The number is reviewer-assigned in `kernel/src/syscall/basic.rs` (S72) and is not a
/// `raven_abi::syscall::Syscall` variant yet. The kernel accepts `GRND_NONBLOCK`/`GRND_RANDOM`,
/// rejects unknown flags with `EINVAL`, and returns `EAGAIN` while the pool is unseeded.
pub fn getrandom(buf: &mut [u8], flags: u32) -> Result<usize, Errno> {
    // SAFETY: `buf` is a live writable slice; the kernel copies at most `buf.len()` bytes into
    // it through the fault-safe user layer.
    let raw = unsafe { arch::syscall3(0x08, buf.as_mut_ptr() as usize, buf.len(), flags as usize) };
    crate::errno::ret(raw)
}

/// Raw `PROC_EXEC` (`0x6a`): replace the caller's image from an 88-byte flat request.
///
/// The request layout is owned by the kernel/`raven-libc-proc`; this wrapper only checks the
/// pointer it passes. On success the kernel does not return to this code.
pub fn proc_exec(request: &[u8; 88]) -> Result<usize, Errno> {
    // SAFETY: `request` is a live 88-byte array and the kernel validates every pointer and
    // length inside it through the fault-safe user-copy layer.
    let raw = unsafe { arch::syscall1(0x6a, request.as_ptr() as usize) };
    crate::errno::ret(raw)
}

/// Raw `USER_COPY` (`0x8E`): fault-safe copy between two ranges of the caller's address space.
///
/// The kernel validates both `UserSlice` ranges, rejects overlapping ranges and a length above
/// 64 KiB with `EINVAL`, and copies through the assembly fixups so an unmapped page is `EFAULT`
/// and never a kernel fault. The destination is unspecified on `EFAULT`; no partial count is
/// returned. The host build has no kernel, so the same contract is served by a fault-safe copy
/// of the calling process (`process_vm_readv`/`process_vm_writev`), which is what the host tests
/// exercise.
pub fn user_copy(dst: *mut u8, src: *const u8, len: usize) -> Result<(), Errno> {
    #[cfg(target_os = "none")]
    {
        // SAFETY: `dst`/`src` are the caller's ranges. The kernel validates both against the
        // user window before it copies, and every page fault inside the copy is fixed up to
        // EFAULT.
        crate::errno::ret(unsafe {
            arch::syscall3(
                raven_abi::syscall::USER_COPY,
                dst as usize,
                src as usize,
                len,
            )
        })
        .map(|_| ())
    }
    #[cfg(not(target_os = "none"))]
    {
        host_copy::user_copy(dst, src, len)
    }
}

/// The host stand-in for the kernel `USER_COPY` fixup path.
///
/// A host test process has no Raven kernel, so the current-address-space copy is done by the
/// host kernel's own fault-safe primitive. The contract is identical: zero length succeeds
/// without touching a pointer, a length above 64 KiB and an overlapping range are `EINVAL`
/// before any byte, and an unmapped, protected or kernel-half range is `EFAULT`.
///
/// The host syscalls are issued directly rather than through [`crate::arch`]: with the
/// `host-fake` feature those stubs dispatch into the in-memory kernel, which has no real
/// address space and cannot model an unmapped page.
#[cfg(not(target_os = "none"))]
mod host_copy {
    use raven_abi::errno::Errno;

    /// Largest accepted `USER_COPY` length (the kernel contract).
    const MAX_USER_COPY: usize = 64 * 1024;

    /// Bytes per fault-safe round trip; the kernel uses a bounded chunk loop too.
    const CHUNK: usize = 512;

    /// One host `struct iovec`.
    #[repr(C)]
    struct IoVec {
        base: *mut u8,
        len: usize,
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const GETPID: usize = 39;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    const GETPID: usize = 172;

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const PROCESS_VM_READV: usize = 310;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    const PROCESS_VM_READV: usize = 270;

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const PROCESS_VM_WRITEV: usize = 311;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    const PROCESS_VM_WRITEV: usize = 271;

    /// Linux `syscall` with six arguments, independent of the `host-fake` dispatch.
    ///
    /// # Safety
    ///
    /// `number` must be a Linux system-call number and the arguments must satisfy that call.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    unsafe fn host_syscall6(
        number: usize,
        a0: usize,
        a1: usize,
        a2: usize,
        a3: usize,
        a4: usize,
        a5: usize,
    ) -> isize {
        let result: isize;
        // SAFETY: the register convention is the Linux x86-64 `syscall` ABI: number in `rax`,
        // arguments in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") number as isize => result,
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

    /// Linux `svc 0` with six arguments, independent of the `host-fake` dispatch.
    ///
    /// # Safety
    ///
    /// `number` must be a Linux system-call number and the arguments must satisfy that call.
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    unsafe fn host_syscall6(
        number: usize,
        a0: usize,
        a1: usize,
        a2: usize,
        a3: usize,
        a4: usize,
        a5: usize,
    ) -> isize {
        let result: usize;
        // SAFETY: the register convention is the Linux aarch64 `svc 0` ABI: number in `x8`,
        // arguments in `x0..x5`, result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                inlateout("x0") a0 => result,
                in("x1") a1,
                in("x2") a2,
                in("x3") a3,
                in("x4") a4,
                in("x5") a5,
                in("x8") number,
                options(nostack),
            );
        }
        result as isize
    }

    pub(super) fn user_copy(dst: *mut u8, src: *const u8, len: usize) -> Result<(), Errno> {
        #[cfg(all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ))]
        {
            linux_user_copy(dst, src, len)
        }
        #[cfg(not(all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )))]
        {
            let _ = (dst, src, len);
            // The fault-safe host primitive is implemented for the Linux gate hosts.
            Err(Errno::ENOSYS)
        }
    }

    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    fn linux_user_copy(dst: *mut u8, src: *const u8, len: usize) -> Result<(), Errno> {
        if len == 0 {
            return Ok(());
        }
        if len > MAX_USER_COPY {
            return Err(Errno::EINVAL);
        }
        let source = src as usize;
        let destination = dst as usize;
        let Some(source_end) = source.checked_add(len) else {
            return Err(Errno::EFAULT);
        };
        let Some(destination_end) = destination.checked_add(len) else {
            return Err(Errno::EFAULT);
        };
        // The kernel rejects overlap (this is not `memmove`); do not let the host copy hide it.
        if source < destination_end && destination < source_end {
            return Err(Errno::EINVAL);
        }
        // SAFETY: `getpid` takes no arguments and has no side effects.
        let pid = unsafe { host_syscall6(GETPID, 0, 0, 0, 0, 0, 0) };
        if pid < 0 {
            return Err(Errno::EFAULT);
        }
        let mut buffer = [0u8; CHUNK];
        let mut offset = 0usize;
        while offset < len {
            let chunk = len.saturating_sub(offset).min(CHUNK);
            let mut local = IoVec {
                base: buffer.as_mut_ptr(),
                len: chunk,
            };
            let remote = IoVec {
                base: src.wrapping_add(offset).cast_mut(),
                len: chunk,
            };
            // SAFETY: `local` names the kernel-side stack buffer and `remote` the caller's
            // source range; `process_vm_readv` returns EFAULT for an unmapped source.
            let read = unsafe {
                host_syscall6(
                    PROCESS_VM_READV,
                    pid as usize,
                    &mut local as *mut IoVec as usize,
                    1,
                    &remote as *const IoVec as usize,
                    1,
                    0,
                )
            };
            if read < 0 || read as usize != chunk {
                return Err(Errno::EFAULT);
            }
            let local = IoVec {
                base: buffer.as_mut_ptr(),
                len: chunk,
            };
            let mut remote = IoVec {
                base: dst.wrapping_add(offset),
                len: chunk,
            };
            // SAFETY: `local` names the kernel-side stack buffer and `remote` the caller's
            // destination range; `process_vm_writev` returns EFAULT for an unmapped or
            // read-only destination.
            let written = unsafe {
                host_syscall6(
                    PROCESS_VM_WRITEV,
                    pid as usize,
                    &local as *const IoVec as usize,
                    1,
                    &mut remote as *mut IoVec as usize,
                    1,
                    0,
                )
            };
            if written < 0 || written as usize != chunk {
                return Err(Errno::EFAULT);
            }
            offset = offset.saturating_add(chunk);
        }
        Ok(())
    }
}

/// User-visible event record written by `EVQ_WAIT`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvqEvent {
    /// Registration cookie.
    pub cookie: u64,
    /// Event mask.
    pub events: u32,
    /// Reserved padding.
    pub pad: u32,
}

/// DMA buffer information returned by `DMA_INFO`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DmaInfo {
    /// Device-visible bus address.
    pub bus_addr: u64,
    /// Buffer length in bytes.
    pub len: u64,
    /// DMA flags (bit 0: not cache-coherent).
    pub flags: u32,
    /// Reserved padding.
    pub pad: u32,
}

/// MSI/MSI-X message filled by `IRQ_CREATE`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IrqInfo {
    /// Allocated vector.
    pub vector: u32,
    /// Message address low dword.
    pub address_lo: u32,
    /// Message address high dword.
    pub address_hi: u32,
    /// Message data.
    pub data: u32,
}

/// Terminate the calling process.
pub fn exit(code: i32) -> ! {
    // SAFETY: EXIT takes only the scalar exit code and never returns.
    unsafe {
        let _ = arch::syscall1(Syscall::EXIT as usize, code as usize);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Give up the CPU.
pub fn yield_now() -> Result<(), Errno> {
    // SAFETY: YIELD takes no arguments.
    crate::errno::ret(unsafe { arch::syscall0(Syscall::YIELD as usize) }).map(|_| ())
}

/// Write bytes to the debug console.
pub fn debug_write(bytes: &[u8]) -> Result<usize, Errno> {
    // SAFETY: DEBUG_WRITE reads `bytes.len()` bytes from the slice's valid pointer.
    crate::errno::ret(unsafe {
        arch::syscall2(
            Syscall::DEBUG_WRITE as usize,
            bytes.as_ptr() as usize,
            bytes.len(),
        )
    })
}

/// Read a clock (`clock::REALTIME` or `clock::MONOTONIC`).
pub fn clock_get(clock_id: u32) -> Result<Timespec, Errno> {
    let mut timespec = Timespec::default();
    // SAFETY: CLOCK_GET writes one `Timespec` into the valid `&mut timespec` pointer.
    crate::errno::ret(unsafe {
        arch::syscall2(
            Syscall::CLOCK_GET as usize,
            clock_id as usize,
            &mut timespec as *mut Timespec as usize,
        )
    })?;
    Ok(timespec)
}

/// Sleep for at least `ns` nanoseconds.
pub fn nanosleep(ns: u64) -> Result<(), Errno> {
    // SAFETY: NANOSLEEP takes only the scalar duration.
    crate::errno::ret(unsafe { arch::syscall1(Syscall::NANOSLEEP as usize, ns as usize) })
        .map(|_| ())
}

/// Return the calling thread's stable 30-bit thread id (`THREAD_SELF`).
pub fn thread_self() -> Result<u32, Errno> {
    // SAFETY: THREAD_SELF takes no arguments and returns the tid directly.
    let value = unsafe { arch::syscall0(raven_abi::syscall::THREAD_SELF) };
    crate::errno::ret(value).and_then(|value| u32::try_from(value).map_err(|_| Errno::EOVERFLOW))
}

/// Read the system identification record.
pub fn uname() -> Result<Uname, Errno> {
    let mut record = Uname::zeroed();
    // SAFETY: UNAME writes one `Uname` into the valid `&mut record` pointer.
    crate::errno::ret(unsafe {
        arch::syscall1(Syscall::UNAME as usize, &mut record as *mut Uname as usize)
    })?;
    Ok(record)
}

/// Read bytes typed on the debug console.
pub fn debug_read(buffer: &mut [u8], timeout_ns: u64) -> Result<usize, Errno> {
    // SAFETY: DEBUG_READ writes at most `buffer.len()` bytes into the valid buffer.
    crate::errno::ret(unsafe {
        arch::syscall3(
            Syscall::DEBUG_READ as usize,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            timeout_ns as usize,
        )
    })
}

/// Create a thread in `proc_` at `entry` with `stack_top` and one argument.
pub fn thread_create(
    proc_: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
    flags: usize,
) -> Result<Handle, Errno> {
    // SAFETY: THREAD_CREATE takes scalar addresses; the kernel validates the entry, stack and
    // flags.
    handle_from_ret(unsafe {
        arch::syscall5(
            Syscall::THREAD_CREATE as usize,
            proc_.0 as usize,
            entry,
            stack_top,
            arg,
            flags,
        )
    })
}

/// Terminate the calling thread.
pub fn thread_exit(code: i32) -> ! {
    // SAFETY: THREAD_EXIT takes only the scalar exit code and never returns.
    unsafe {
        let _ = arch::syscall1(Syscall::THREAD_EXIT as usize, code as usize);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Wait on a futex word while it equals `expected`.
pub fn futex_wait(addr: &AtomicU32, expected: u32, timeout_ns: u64) -> Result<(), Errno> {
    // SAFETY: FUTEX with WAIT reads the `AtomicU32` behind the valid reference.
    crate::errno::ret(unsafe {
        arch::syscall4(
            Syscall::FUTEX as usize,
            addr.as_ptr() as usize,
            futex::WAIT as usize,
            expected as usize,
            timeout_ns as usize,
        )
    })
    .map(|_| ())
}

/// Wake up to `count` futex waiters.
pub fn futex_wake(addr: &AtomicU32, count: u32) -> Result<usize, Errno> {
    // SAFETY: FUTEX with WAKE reads the `AtomicU32` behind the valid reference.
    crate::errno::ret(unsafe {
        arch::syscall3(
            Syscall::FUTEX as usize,
            addr.as_ptr() as usize,
            futex::WAKE as usize,
            count as usize,
        )
    })
}

/// Create an empty process and return a handle with all rights.
pub fn proc_create(flags: usize) -> Result<Handle, Errno> {
    // SAFETY: PROC_CREATE takes only scalar flags.
    handle_from_ret(unsafe { arch::syscall1(Syscall::PROC_CREATE as usize, flags) })
}

/// Raw `proc_set_command(proc, name, len)` number (P2o/P2r).
pub const PROC_SET_COMMAND: usize = 0x45;

/// Set the kernel command name for `proc_`.
///
/// The name is UTF-8 without NUL and at most 256 bytes; the kernel truncates at a UTF-8
/// boundary. The target is `SELF_PROC` or a process handle with `ADMIN`.
pub fn proc_set_command(proc_: Handle, name: &[u8]) -> Result<(), Errno> {
    // SAFETY: PROC_SET_COMMAND reads `name.len()` bytes from the valid slice; the kernel
    // validates the process handle.
    crate::errno::ret(unsafe {
        arch::syscall3(
            PROC_SET_COMMAND,
            proc_.0 as usize,
            name.as_ptr() as usize,
            name.len(),
        )
    })
    .map(|_| ())
}

/// Wait for `proc_` to exit and return its exit code.
pub fn proc_wait(proc_: Handle) -> Result<i32, Errno> {
    let mut code = 0i32;
    // SAFETY: PROC_WAIT writes one `i32` into the valid `&mut code` pointer.
    crate::errno::ret(unsafe {
        arch::syscall2(
            Syscall::PROC_WAIT as usize,
            proc_.0 as usize,
            &mut code as *mut i32 as usize,
        )
    })?;
    Ok(code)
}

/// Terminate `proc_` and all its threads; requires `Rights::SIGNAL`.
pub fn proc_terminate(proc_: Handle, code: i32) -> Result<(), Errno> {
    // SAFETY: PROC_TERMINATE takes scalar arguments only.
    crate::errno::ret(unsafe {
        arch::syscall2(
            Syscall::PROC_TERMINATE as usize,
            proc_.0 as usize,
            code as usize,
        )
    })
    .map(|_| ())
}

/// Add, remove or clear namespace prefixes on `proc_`.
pub fn proc_namespace(proc_: Handle, op: u32, prefix: &[u8]) -> Result<(), Errno> {
    // SAFETY: PROC_NAMESPACE reads `prefix.len()` bytes from the valid slice.
    crate::errno::ret(unsafe {
        arch::syscall4(
            Syscall::PROC_NAMESPACE as usize,
            proc_.0 as usize,
            op as usize,
            prefix.as_ptr() as usize,
            prefix.len(),
        )
    })
    .map(|_| ())
}

/// Map memory into `proc_` and return the address.
///
/// `prot` is a `prot::*` mask; `flags` a `map::*` mask.
pub fn map(
    proc_: Handle,
    memobj: Handle,
    offset: usize,
    addr: usize,
    len: usize,
    prot: u32,
    flags: u32,
) -> Result<usize, Errno> {
    // SAFETY: MAP takes scalar arguments only; the kernel validates the handle, rights, range
    // and flags. The address is decoded with `from_ret`, never with `ret < 0`.
    crate::errno::ret(unsafe {
        arch::syscall6(
            Syscall::MAP as usize,
            proc_.0 as usize,
            memobj.0 as usize,
            offset,
            addr,
            len,
            map_flags::pack(prot as usize, flags as usize),
        )
    })
}

/// Unmap a range in `proc_`.
pub fn unmap(proc_: Handle, addr: usize, len: usize) -> Result<(), Errno> {
    // SAFETY: UNMAP takes scalar arguments only.
    crate::errno::ret(unsafe {
        arch::syscall3(Syscall::UNMAP as usize, proc_.0 as usize, addr, len)
    })
    .map(|_| ())
}

/// Change the protection of a range in `proc_`.
pub fn protect(proc_: Handle, addr: usize, len: usize, prot: u32) -> Result<(), Errno> {
    // SAFETY: PROTECT takes scalar arguments only.
    crate::errno::ret(unsafe {
        arch::syscall4(
            Syscall::PROTECT as usize,
            proc_.0 as usize,
            addr,
            len,
            prot as usize,
        )
    })
    .map(|_| ())
}

/// Flush dirty shared pages in `[addr, addr + len)`.
pub fn msync(proc_: Handle, addr: usize, len: usize, flags: u32) -> Result<(), Errno> {
    // SAFETY: MSYNC takes scalar arguments only; the kernel validates the range and flags.
    crate::errno::ret(unsafe {
        arch::syscall4(
            raven_abi::syscall::MSYNC,
            proc_.0 as usize,
            addr,
            len,
            flags as usize,
        )
    })
    .map(|_| ())
}

/// Create a memory object of `len` bytes.
pub fn memobj_create(len: usize, flags: usize) -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes scalar arguments only.
    handle_from_ret(unsafe { arch::syscall2(Syscall::MEMOBJ_CREATE as usize, len, flags) })
}

/// Supply one page to a pager-backed memory object.
pub fn pager_supply(object: Handle, offset: usize, data: &[u8]) -> Result<(), Errno> {
    // SAFETY: PAGER_SUPPLY reads `data.len()` bytes from the valid slice.
    crate::errno::ret(unsafe {
        arch::syscall4(
            Syscall::PAGER_SUPPLY as usize,
            object.0 as usize,
            offset,
            data.as_ptr() as usize,
            data.len(),
        )
    })
    .map(|_| ())
}

/// Close a handle.
pub fn handle_close(handle: Handle) -> Result<(), Errno> {
    // SAFETY: HANDLE_CLOSE takes a scalar handle value.
    crate::errno::ret(unsafe { arch::syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize) })
        .map(|_| ())
}

/// Duplicate a handle with an intersected rights mask.
pub fn handle_dup(handle: Handle, rights: Rights) -> Result<Handle, Errno> {
    // SAFETY: HANDLE_DUP takes scalar arguments only.
    handle_from_ret(unsafe {
        arch::syscall2(
            Syscall::HANDLE_DUP as usize,
            handle.0 as usize,
            rights.bits() as usize,
        )
    })
}

/// Install a duplicate of `source` in `dest` with an intersected rights mask.
pub fn handle_transfer(dest: Handle, source: Handle, rights: Rights) -> Result<Handle, Errno> {
    if dest == Handle::NONE || source == Handle::NONE || rights == Rights::NONE {
        return Err(Errno::EINVAL);
    }
    // SAFETY: HANDLE_TRANSFER takes scalar arguments only.
    handle_from_ret(unsafe {
        arch::syscall3(
            Syscall::HANDLE_TRANSFER as usize,
            dest.0 as usize,
            source.0 as usize,
            rights.bits() as usize,
        )
    })
}

/// Remove `rights` from `handle` and every handle derived from it.
pub fn handle_revoke(handle: Handle, rights: Rights) -> Result<(), Errno> {
    // SAFETY: HANDLE_REVOKE takes scalar arguments only.
    crate::errno::ret(unsafe {
        arch::syscall2(
            Syscall::HANDLE_REVOKE as usize,
            handle.0 as usize,
            rights.bits() as usize,
        )
    })
    .map(|_| ())
}

/// Create an event queue.
pub fn evq_create(flags: u32) -> Result<Handle, Errno> {
    // SAFETY: EVQ_CREATE takes scalar flags.
    handle_from_ret(unsafe { arch::syscall1(Syscall::EVQ_CREATE as usize, flags as usize) })
}

/// Add, modify or delete an event-queue registration.
pub fn evq_ctl(
    evq: Handle,
    op: u32,
    handle: Handle,
    events: u32,
    cookie: u64,
) -> Result<(), Errno> {
    // SAFETY: EVQ_CTL takes scalar arguments only.
    crate::errno::ret(unsafe {
        arch::syscall5(
            Syscall::EVQ_CTL as usize,
            evq.0 as usize,
            op as usize,
            handle.0 as usize,
            events as usize,
            cookie as usize,
        )
    })
    .map(|_| ())
}

/// Wait for events and fill `out`; returns the number of records written.
pub fn evq_wait(evq: Handle, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno> {
    // SAFETY: EVQ_WAIT writes at most `out.len()` records into the valid output slice.
    crate::errno::ret(unsafe {
        arch::syscall4(
            Syscall::EVQ_WAIT as usize,
            evq.0 as usize,
            out.as_mut_ptr() as usize,
            out.len(),
            timeout_ns as usize,
        )
    })
}

/// Create a timer object.
pub fn timer_create(clock_id: u32, flags: u32) -> Result<Handle, Errno> {
    // SAFETY: TIMER_CREATE takes scalar arguments only.
    handle_from_ret(unsafe {
        arch::syscall2(
            Syscall::TIMER_CREATE as usize,
            clock_id as usize,
            flags as usize,
        )
    })
}

/// Create a user event.
pub fn event_create(flags: u32) -> Result<Handle, Errno> {
    // SAFETY: EVENT_CREATE takes scalar flags.
    handle_from_ret(unsafe { arch::syscall1(Syscall::EVENT_CREATE as usize, flags as usize) })
}

/// Signal a user event with `value`.
pub fn event_signal(handle: Handle, value: u64) -> Result<(), Errno> {
    // SAFETY: EVENT_SIGNAL takes scalar arguments only.
    crate::errno::ret(unsafe {
        arch::syscall2(
            Syscall::EVENT_SIGNAL as usize,
            handle.0 as usize,
            value as usize,
        )
    })
    .map(|_| ())
}

/// Arm or disarm a timer.
pub fn timer_set(timer: Handle, initial_ns: u64, interval_ns: u64) -> Result<(), Errno> {
    // SAFETY: TIMER_SET takes scalar arguments only.
    crate::errno::ret(unsafe {
        arch::syscall3(
            Syscall::TIMER_SET as usize,
            timer.0 as usize,
            initial_ns as usize,
            interval_ns as usize,
        )
    })
    .map(|_| ())
}

/// Register the calling process as the `name:` scheme server.
pub fn scheme_register(name: &[u8]) -> Result<(), Errno> {
    // SAFETY: SCHEME_REGISTER reads `name.len()` bytes and passes the self-process handle.
    crate::errno::ret(unsafe {
        arch::syscall3(
            Syscall::SCHEME_REGISTER as usize,
            name.as_ptr() as usize,
            name.len(),
            Handle::SELF_PROC.0 as usize,
        )
    })
    .map(|_| ())
}

/// Raw `PIPE2` syscall number (`pipe2(fds, flags)`); owned by the kernel because
/// `crates/raven-abi` is a contract crate and does not expose the raw pipe numbers.
const PIPE2: usize = 0x6f;

/// `pipe2(fds, flags)`: create an in-kernel pipe and return its two handles.
///
/// The kernel validates the flags (`O_CLOEXEC`/`O_NONBLOCK`) and writes two handle ids into the
/// caller's 8-byte buffer. This wrapper is the only safe entry point for the platform crate.
pub fn pipe2(flags: u32) -> Result<(Handle, Handle), Errno> {
    let mut fds = [0u32; 2];
    // SAFETY: PIPE2 writes exactly two u32 handle ids into the live 8-byte buffer.
    crate::errno::ret(unsafe { arch::syscall2(PIPE2, fds.as_mut_ptr() as usize, flags as usize) })?;
    let [read, write] = fds;
    Ok((Handle(read), Handle(write)))
}

/// Open `path` (bytes, no NUL, no UTF-8 requirement at this layer).
pub fn open(path: &[u8], flags: u32, mode: u32) -> Result<Handle, Errno> {
    // SAFETY: OPEN reads `path.len()` bytes from the valid slice.
    handle_from_ret(unsafe {
        arch::syscall4(
            Syscall::OPEN as usize,
            path.as_ptr() as usize,
            path.len(),
            flags as usize,
            mode as usize,
        )
    })
}

/// Swap the process file-creation mask; returns the previous value.
pub fn umask(mask: u32) -> Result<u32, Errno> {
    // SAFETY: PROC_UMASK takes one scalar argument and has no pointer.
    let previous = crate::errno::ret(unsafe {
        arch::syscall1(raven_abi::syscall::PROC_UMASK, mask as usize)
    })?;
    u32::try_from(previous).map_err(|_| Errno::EOVERFLOW)
}

/// Read from a handle.
pub fn read(handle: Handle, buf: &mut [u8]) -> Result<usize, Errno> {
    // SAFETY: READ writes at most `buf.len()` bytes into the valid buffer.
    crate::errno::ret(unsafe {
        arch::syscall3(
            Syscall::READ as usize,
            handle.0 as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
        )
    })
}

/// Write to a handle.
pub fn write(handle: Handle, buf: &[u8]) -> Result<usize, Errno> {
    // SAFETY: WRITE reads `buf.len()` bytes from the valid buffer.
    crate::errno::ret(unsafe {
        arch::syscall3(
            Syscall::WRITE as usize,
            handle.0 as usize,
            buf.as_ptr() as usize,
            buf.len(),
        )
    })
}

/// Close a scheme file.
pub fn close(handle: Handle) -> Result<(), Errno> {
    // SAFETY: CLOSE takes a scalar handle.
    crate::errno::ret(unsafe { arch::syscall1(Syscall::CLOSE as usize, handle.0 as usize) })
        .map(|_| ())
}

/// Duplicate a scheme file with a rights mask.
pub fn dup(handle: Handle, rights: u32) -> Result<Handle, Errno> {
    // SAFETY: DUP takes scalar arguments only.
    handle_from_ret(unsafe {
        arch::syscall2(Syscall::DUP as usize, handle.0 as usize, rights as usize)
    })
}

/// A new handle for the caller's controlling terminal.
///
/// Native `PROC_CTTY_HANDLE` (kernel `syscall/tty.rs`, `0x1e3`). `/dev/tty` names this live
/// terminal, not a VFS node.
pub fn ctty_handle() -> Result<Handle, Errno> {
    // SAFETY: no arguments and no user pointer.
    handle_from_ret(unsafe { arch::syscall0(0x1e3) })
}

/// Read the 64-byte `FSTAT` record.
pub fn fstat(handle: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
    // SAFETY: FSTAT writes at most 64 bytes into the valid output record.
    crate::errno::ret(unsafe {
        arch::syscall3(
            Syscall::FSTAT as usize,
            handle.0 as usize,
            out.as_mut_ptr() as usize,
            out.len(),
        )
    })
    .map(|_| ())
}

/// Apply a POSIX byte-range lock operation to `handle`.
///
/// `command` is one of [`raven_abi::types::fcntl::GETLK`], [`raven_abi::types::fcntl::SETLK`]
/// or [`raven_abi::types::fcntl::SETLKW`]. The kernel reads the input lock and, for `GETLK`,
/// writes the first conflicting record back into `lock`.
pub fn record_lock(handle: Handle, command: u32, lock: &mut RecordLock) -> Result<(), Errno> {
    // SAFETY: RECORD_LOCK reads the 32-byte input record and, for GETLK, writes the same
    // 32-byte record back; `lock` is a live, aligned `RecordLock`.
    crate::errno::ret(unsafe {
        arch::syscall3(
            raven_abi::syscall::RECORD_LOCK,
            handle.0 as usize,
            command as usize,
            lock as *mut RecordLock as usize,
        )
    })
    .map(|_| ())
}

/// Move a file handle's cursor and return the new offset (`SEEK`, raw number `0x6D`).
pub fn seek(handle: Handle, offset: i64, whence: u32) -> Result<u64, Errno> {
    // SAFETY: SEEK takes a scalar handle, a signed offset and a scalar whence.
    let result = crate::errno::ret(unsafe {
        arch::syscall6(
            0x6D,
            handle.0 as usize,
            offset as isize as usize,
            whence as usize,
            0,
            0,
            0,
        )
    })?;
    u64::try_from(result).map_err(|_| Errno::EOVERFLOW)
}

/// Send a typed `CALL` request and return the response length.
pub fn call(handle: Handle, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
    // SAFETY: CALL reads the request slice and writes at most `response.len()` bytes.
    crate::errno::ret(unsafe {
        arch::syscall6(
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
    // SAFETY: RECV writes one packet into the valid buffer.
    handle_from_ret(unsafe {
        arch::syscall2(
            Syscall::RECV as usize,
            packet.as_mut_ptr() as usize,
            packet.len(),
        )
    })
}

/// Send one response packet for a server-side request handle.
pub fn send(request: Handle, packet: &[u8]) -> Result<(), Errno> {
    // SAFETY: SEND reads one encoded packet from the valid slice.
    crate::errno::ret(unsafe {
        arch::syscall3(
            Syscall::SEND as usize,
            request.0 as usize,
            packet.as_ptr() as usize,
            packet.len(),
        )
    })
    .map(|_| ())
}

/// Create an MMIO region through the root device capability.
pub fn mmio_create(root: Handle, phys: u64, len: usize, attrs: u32) -> Result<Handle, Errno> {
    // SAFETY: MMIO_CREATE takes scalar arguments only.
    handle_from_ret(unsafe {
        arch::syscall4(
            Syscall::MMIO_CREATE as usize,
            root.0 as usize,
            phys as usize,
            len,
            attrs as usize,
        )
    })
}

/// Return a read-only boot blob.
pub fn boot_blob(root: Handle, kind: u32) -> Result<Handle, Errno> {
    // SAFETY: BOOT_BLOB takes scalar arguments only.
    handle_from_ret(unsafe {
        arch::syscall2(Syscall::BOOT_BLOB as usize, root.0 as usize, kind as usize)
    })
}

/// Allocate a device-visible DMA buffer.
pub fn dma_alloc(
    root: Handle,
    len: usize,
    align: usize,
    mask_bits: u32,
    flags: u32,
) -> Result<Handle, Errno> {
    // SAFETY: DMA_ALLOC takes scalar arguments only.
    handle_from_ret(unsafe {
        arch::syscall5(
            Syscall::DMA_ALLOC as usize,
            root.0 as usize,
            len,
            align,
            mask_bits as usize,
            flags as usize,
        )
    })
}

/// Read the DMA information record.
pub fn dma_info(buffer: Handle) -> Result<DmaInfo, Errno> {
    let mut info = DmaInfo::default();
    // SAFETY: DMA_INFO writes one `DmaInfo` into the valid stack record.
    crate::errno::ret(unsafe {
        arch::syscall2(
            Syscall::DMA_INFO as usize,
            buffer.0 as usize,
            &mut info as *mut DmaInfo as usize,
        )
    })?;
    Ok(info)
}

/// Create an interrupt source.
pub fn irq_create(
    root: Handle,
    kind: u32,
    number: u32,
    info: Option<&mut IrqInfo>,
) -> Result<Handle, Errno> {
    let pointer = match info {
        Some(info) => info as *mut IrqInfo as usize,
        None => 0,
    };
    // SAFETY: IRQ_CREATE takes scalar arguments and an optional writable record.
    handle_from_ret(unsafe {
        arch::syscall4(
            Syscall::IRQ_CREATE as usize,
            root.0 as usize,
            kind as usize,
            number as usize,
            pointer,
        )
    })
}

/// Wait for an interrupt and return the accumulated firing count.
pub fn irq_wait(irq: Handle, timeout_ns: u64) -> Result<usize, Errno> {
    // SAFETY: IRQ_WAIT takes scalar arguments only.
    crate::errno::ret(unsafe {
        arch::syscall2(
            Syscall::IRQ_WAIT as usize,
            irq.0 as usize,
            timeout_ns as usize,
        )
    })
}

/// Re-arm a level-triggered interrupt.
pub fn irq_ack(irq: Handle) -> Result<(), Errno> {
    // SAFETY: IRQ_ACK takes a scalar handle.
    crate::errno::ret(unsafe { arch::syscall1(Syscall::IRQ_ACK as usize, irq.0 as usize) })
        .map(|_| ())
}

fn handle_from_ret(value: isize) -> Result<Handle, Errno> {
    let value = Errno::from_ret(value)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Name of the typed wrapper for `call`, or the empty string when there is none.
///
/// The coverage test iterates `Syscall::ALL` and fails when a variant has no wrapper.
pub const fn wrapper_name(call: Syscall) -> &'static str {
    match call {
        Syscall::EXIT => "exit",
        Syscall::YIELD => "yield_now",
        Syscall::DEBUG_WRITE => "debug_write",
        Syscall::CLOCK_GET => "clock_get",
        Syscall::NANOSLEEP => "nanosleep",
        Syscall::UNAME => "uname",
        Syscall::DEBUG_READ => "debug_read",
        Syscall::THREAD_CREATE => "thread_create",
        Syscall::THREAD_EXIT => "thread_exit",
        Syscall::FUTEX => "futex_wait/futex_wake",
        Syscall::PROC_CREATE => "proc_create",
        Syscall::PROC_WAIT => "proc_wait",
        Syscall::PROC_TERMINATE => "proc_terminate",
        Syscall::PROC_NAMESPACE => "proc_namespace",
        Syscall::MAP => "map",
        Syscall::UNMAP => "unmap",
        Syscall::PROTECT => "protect",
        Syscall::MEMOBJ_CREATE => "memobj_create",
        Syscall::PAGER_SUPPLY => "pager_supply",
        Syscall::HANDLE_CLOSE => "handle_close",
        Syscall::HANDLE_DUP => "handle_dup",
        Syscall::HANDLE_TRANSFER => "handle_transfer",
        Syscall::HANDLE_REVOKE => "handle_revoke",
        Syscall::EVQ_CREATE => "evq_create",
        Syscall::EVQ_CTL => "evq_ctl",
        Syscall::EVQ_WAIT => "evq_wait",
        Syscall::TIMER_CREATE => "timer_create",
        Syscall::EVENT_CREATE => "event_create",
        Syscall::EVENT_SIGNAL => "event_signal",
        Syscall::TIMER_SET => "timer_set",
        Syscall::SCHEME_REGISTER => "scheme_register",
        Syscall::OPEN => "open",
        Syscall::READ => "read",
        Syscall::WRITE => "write",
        Syscall::CLOSE => "close",
        Syscall::DUP => "dup",
        Syscall::FSTAT => "fstat",
        Syscall::CALL => "call",
        Syscall::RECV => "recv",
        Syscall::SEND => "send",
        Syscall::MMIO_CREATE => "mmio_create",
        Syscall::BOOT_BLOB => "boot_blob",
        Syscall::DMA_ALLOC => "dma_alloc",
        Syscall::DMA_INFO => "dma_info",
        Syscall::IRQ_CREATE => "irq_create",
        Syscall::IRQ_WAIT => "irq_wait",
        Syscall::IRQ_ACK => "irq_ack",
    }
}

const _: () = {
    assert!(WRAPPER_COUNT == 47);
    assert!(prot::MASK == 7);
};

#[cfg(test)]
#[path = "sys_ut.rs"]
mod tests;
