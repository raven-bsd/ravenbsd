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
use raven_abi::types::{Handle, Rights, Timespec, futex, map as map_flags, prot};
use raven_abi::uname::Uname;

use crate::arch;

/// Number of typed wrappers in this module; must equal `Syscall::ALL.len()`.
pub const WRAPPER_COUNT: usize = 47;

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

/// Open `path` (bytes, no NUL, no UTF-8 requirement at this layer).
pub fn open(path: &[u8], flags: u32) -> Result<Handle, Errno> {
    // SAFETY: OPEN reads `path.len()` bytes from the valid slice.
    handle_from_ret(unsafe {
        arch::syscall3(
            Syscall::OPEN as usize,
            path.as_ptr() as usize,
            path.len(),
            flags as usize,
        )
    })
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
