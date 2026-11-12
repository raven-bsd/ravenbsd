// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Native syscall numbers and argument conventions (ABI v1, milestone M1 set).
//!
//! Register conventions (`architecture/01-kernel.md` section 3):
//! x86-64 `syscall`: number `rax`, args `rdi rsi rdx r10 r8 r9`, result `rax`;
//! aarch64 `svc #0`: number `x8`, args `x0..x5`, result `x0`;
//! riscv64 `ecall`: number `a7`, args `a0..a5`, result `a0`.
//! A result `>= 0` is success; `-errno` is failure (see [`crate::errno`]).
//!
//! Number ranges: `0x01..` basics, `0x20..` threads/futex, `0x30..` process control,
//! `0x40..` memory, `0x50..` handles, `0x60..` scheme IPC (reserved), `0x70..` driver access
//! (reserved), `0x80..` signals (reserved), `0x100..` pager and future (reserved).

macro_rules! syscalls {
    ($($(#[$doc:meta])* $name:ident = $val:expr;)*) => {
        /// A native syscall number. Variant names mirror the documented `SYS_*` names.
        #[allow(non_camel_case_types)]
        #[repr(usize)]
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum Syscall {
            $($(#[$doc])* $name = $val,)*
        }

        impl Syscall {
            /// All defined syscalls, in declaration order.
            pub const ALL: &'static [Syscall] = &[$(Syscall::$name,)*];

            /// Convert a raw number from user space.
            pub const fn from_usize(n: usize) -> Option<Syscall> {
                match n {
                    $($val => Some(Syscall::$name),)*
                    _ => None,
                }
            }
        }
    };
}

syscalls! {
    /// `exit(code: i32) -> !` terminates the calling process (all its threads).
    EXIT = 0x01;
    /// `yield() -> 0` gives up the CPU.
    YIELD = 0x02;
    /// `debug_write(ptr: *const u8, len: usize) -> bytes_written` writes to the debug console.
    /// Requires the `ADMIN` right on the caller's process handle in later versions; open in M1.
    DEBUG_WRITE = 0x03;
    /// `clock_get(clock_id: u32, out: *mut Timespec) -> 0`. See [`crate::types::clock`].
    CLOCK_GET = 0x04;
    /// `nanosleep(ns: u64) -> 0` sleeps at least `ns` nanoseconds (may return `-EINTR` later).
    NANOSLEEP = 0x05;
    /// `uname(out: *mut Uname) -> 0` fills the system identification record.
    UNAME = 0x06;
    /// `debug_read(buf: *mut u8, len: usize, timeout_ns: u64) -> isize` reads bytes typed on the debug console (serial input).
    /// Blocks until at least one byte is available or `timeout_ns` (0 = wait forever) expires (returns 0 on timeout); returns
    /// the number of bytes stored (at most `len`). Open in this version; will need `ADMIN` like `DEBUG_WRITE`. Ticket S27.
    DEBUG_READ = 0x07;

    /// `thread_create(proc: Handle, entry: usize, stack_top: usize, arg: usize, flags: usize) -> Handle`
    /// creates a thread in `proc` starting at `entry` with the stack pointer at `stack_top`;
    /// `arg` is passed in the first argument register (`rdi`/`x0`/`a0`).
    THREAD_CREATE = 0x20;
    /// `thread_exit(code: i32) -> !` terminates the calling thread.
    THREAD_EXIT = 0x21;
    /// `futex(addr: *const u32, op: u32, val: u32, timeout_ns: u64) -> isize`. `op` is
    /// [`crate::types::futex::WAIT`] (sleep while `*addr == val`; `-EAGAIN` if it differs, `-ETIMEDOUT` on
    /// timeout, `timeout_ns == 0` means no timeout) or [`crate::types::futex::WAKE`] (wake up to `val`
    /// waiters, returns the number woken).
    FUTEX = 0x22;

    /// `proc_create(flags: usize) -> Handle` creates an empty process (no threads) and returns a
    /// handle with all rights.
    PROC_CREATE = 0x30;
    /// `proc_wait(proc: Handle, out_code: *mut i32) -> 0` blocks until the process has exited and stores its exit code.
    PROC_WAIT = 0x31;
    /// `proc_terminate(proc: Handle, code: i32) -> 0` terminates `proc` and all its threads; requires
    /// `Rights::SIGNAL` on the process handle. The exit code is stored for `PROC_WAIT`. `EBADF` for an
    /// invalid/stale handle, `EPERM` without `SIGNAL`, `ESRCH` when the process is already terminal.
    /// Terminating self is allowed and behaves like `exit(code)`. See contract C16.
    PROC_TERMINATE = 0x32;
    /// `proc_namespace(proc: Handle, op: u32, prefix: &str) -> 0` adds, removes or clears namespace prefixes; `proc` is
    /// `SELF_PROC` or a process handle with `ADMIN`. Child prefixes must be inside the caller's namespace (`ADMIN` root may
    /// grant any valid prefix). `op` is `types::namespace::{ADD,REMOVE,CLEAR}`; see contract C17.
    PROC_NAMESPACE = 0x33;

    /// `map(proc: Handle, memobj: Handle, offset: usize, addr: usize, len: usize, flags: usize) -> addr`
    /// maps `len` bytes (page multiple) of `memobj` (or fresh zeroed memory when `memobj ==
    /// Handle::NONE`) at `addr` (hint, or exact with `MAP_FIXED`) in `proc`. `flags` encodes
    /// `prot | (map_flags << 8)`, see [`crate::types::prot`] and [`crate::types::map`].
    MAP = 0x40;
    /// `unmap(proc: Handle, addr: usize, len: usize) -> 0`.
    UNMAP = 0x41;
    /// `protect(proc: Handle, addr: usize, len: usize, prot: usize) -> 0`.
    PROTECT = 0x42;

    /// `memobj_create(len: usize, flags: usize) -> Handle` creates an anonymous, zero-filled memory object of `len` bytes
    /// (page multiple, `flags` must be 0) with rights `READ | WRITE | MAP | DUP | TRANSFER`. Map it into any process
    /// with [`Syscall::MAP`] (`map::SHARED` shares the frames, otherwise copy-on-write). This is how a parent
    /// writes a child's stack and segments: map the object into itself, fill it, map the same object into the child.
    MEMOBJ_CREATE = 0x43;

    /// `handle_close(h: Handle) -> 0`.
    HANDLE_CLOSE = 0x50;
    /// `handle_dup(h: Handle, rights_mask: u32) -> Handle`; new rights = old rights AND mask. Requires `DUP`.
    HANDLE_DUP = 0x51;
    /// `handle_transfer(dest_proc: Handle, h: Handle, rights_mask: u32) -> Handle` installs a duplicate of `h` in the handle table of
    /// the process `dest_proc` (which needs the `WRITE` right) and returns the handle value valid IN THE DESTINATION. `h` needs
    /// `TRANSFER`; new rights = old rights AND mask (rights never grow). Used for spawn-time grants (before the child's first thread
    /// runs) and by servers. Ticket S25.
    HANDLE_TRANSFER = 0x52;
    /// `handle_revoke(h: Handle, rights_mask: u32) -> 0` removes `rights_mask` (0 = all held) from `h` and every handle derived
    /// from it, including handles in other processes. The handle stays open with the remaining rights; a later operation that needs
    /// a revoked right fails `EPERM` (a right never granted stays `EACCES`). Repeating a revoke is a no-op; a stale handle is
    /// `EBADF`. See contract C17.
    HANDLE_REVOKE = 0x53;

    /// `evq_create(flags: u32) -> Handle` creates an event queue (wait-for-many). Ticket S26.
    EVQ_CREATE = 0x58;
    /// `evq_ctl(evq: Handle, op: u32, h: Handle, events: u32, cookie: u64) -> 0`; `op` ADD=1, MOD=2, DEL=3; `events` is a mask of
    /// READABLE=1, WRITABLE=2, HUP=4, SIGNALED=8 (timer expired, event signalled, IRQ fired) plus EDGE=0x8000_0000 for edge-triggered. Ticket S26.
    EVQ_CTL = 0x59;
    /// `evq_wait(evq: Handle, out: *mut EvqEvent, max: usize, timeout_ns: u64) -> count`; `EvqEvent` is `{ cookie: u64, events: u32, _pad: u32 }`;
    /// `timeout_ns == 0` waits forever, a timeout returns 0. Ticket S26.
    EVQ_WAIT = 0x5A;
    /// `timer_create(clock_id: u32, flags: u32) -> Handle` creates a disarmed timer object usable as an event source. Ticket S26.
    TIMER_CREATE = 0x5B;
    /// `event_create(flags: u32) -> Handle` creates a user event (doorbell) that another process can signal. Ticket S26.
    EVENT_CREATE = 0x5C;
    /// `event_signal(h: Handle, value: u64) -> 0` sets the signalled state of a user event and wakes its waiters. Ticket S26.
    EVENT_SIGNAL = 0x5D;
    /// `timer_set(timer: Handle, initial_ns: u64, interval_ns: u64) -> 0` arms (`initial_ns != 0`) or disarms (0) a timer; `interval_ns != 0` repeats. Ticket S26.
    TIMER_SET = 0x5E;

    /// `scheme_register(name: &str, handle: Handle) -> 0`; register a scheme server handle under a name.
    SCHEME_REGISTER = 0x60;
    /// `open(path: &str, flags: u32, mode: u32) -> Handle`; scheme-routed open (file/scheme namespace).
    OPEN = 0x61;
    /// `read(h: Handle, buf: &mut [u8]) -> usize`.
    READ = 0x62;
    /// `write(h: Handle, buf: &[u8]) -> usize`.
    WRITE = 0x63;
    /// `close(h: Handle) -> 0`.
    CLOSE = 0x64;
    /// `dup(h: Handle, rights_mask: u32) -> Handle` (scheme-file variant of HANDLE_DUP).
    DUP = 0x65;
    /// `fstat(h: Handle, buf: &mut Stat) -> 0`.
    FSTAT = 0x66;
    /// `call(h: Handle, request: &[u8]) -> (u32, response buffer)`; typed RPC escape hatch.
    CALL = 0x67;
    /// Server side: `recv() -> (Handle, packet)`; receive the next scheme packet.
    RECV = 0x68;
    /// Server side: `send(handle: Handle, packet: &[u8]) -> 0`; reply to a packet.
    SEND = 0x69;

    /// `mmio_create(root_dev: Handle, phys: u64, len: usize, attrs: u32) -> Handle` returns an `MmioRegion` (map it with [`Syscall::MAP`]);
    /// needs the `ADMIN` root device capability (`Handle::ROOT_DEVICE`); RAM and kernel memory are refused. Ticket S21a.
    MMIO_CREATE = 0x70;
    /// `boot_blob(root_dev: Handle, kind: u32) -> Handle` returns a read-only memory object with a boot-time blob: kind 1 = ACPI tables
    /// bundle, 2 = device tree (DTB), 3 = memory map summary. Ticket S21a.
    BOOT_BLOB = 0x71;
    /// `dma_alloc(root_dev: Handle, len: usize, align: usize, mask_bits: u32, flags: u32) -> Handle` allocates device-visible memory
    /// (physically contiguous, bus address below `1 << mask_bits`); freed on close or process exit. Ticket S21b.
    DMA_ALLOC = 0x72;
    /// `dma_info(h: Handle, out: *mut DmaInfo) -> 0`; `DmaInfo` is `{ bus_addr: u64, len: u64, flags: u32, _pad: u32 }`. Ticket S21b.
    DMA_INFO = 0x73;
    /// `irq_create(root_dev: Handle, kind: u32, number: u32, flags: u32) -> Handle`; kind LEGACY=1 (IOAPIC pin), MSI=2, MSIX=3 (vector allocation). Ticket S21c.
    IRQ_CREATE = 0x74;
    /// `irq_wait(irq: Handle, timeout_ns: u64) -> count` blocks until the interrupt fired since the last wait/ack, returns the pending count. Ticket S21c.
    IRQ_WAIT = 0x75;
    /// `irq_ack(irq: Handle) -> 0` re-enables (unmasks) the interrupt after the driver handled it. Ticket S21c.
    IRQ_ACK = 0x76;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Handle;

    #[test]
    fn numbers_are_unique_and_roundtrip() {
        for (i, a) in Syscall::ALL.iter().enumerate() {
            assert_eq!(Syscall::from_usize(*a as usize), Some(*a));
            for b in &Syscall::ALL[i + 1..] {
                assert_ne!(*a as usize, *b as usize, "{a:?} and {b:?} share a number");
            }
        }
    }

    #[test]
    fn unknown_numbers_are_none() {
        assert_eq!(Syscall::from_usize(0), None);
        assert_eq!(Syscall::from_usize(0xdead), None);
    }

    #[test]
    fn m1_numbers_are_frozen() {
        assert_eq!(Syscall::EXIT as usize, 0x01);
        assert_eq!(Syscall::DEBUG_WRITE as usize, 0x03);
        assert_eq!(Syscall::UNAME as usize, 0x06);
        assert_eq!(Syscall::FUTEX as usize, 0x22);
        assert_eq!(Syscall::PROC_CREATE as usize, 0x30);
        assert_eq!(Syscall::PROC_WAIT as usize, 0x31);
        assert_eq!(Syscall::MAP as usize, 0x40);
        assert_eq!(Syscall::MEMOBJ_CREATE as usize, 0x43);
        assert_eq!(Syscall::HANDLE_DUP as usize, 0x51);
    }

    #[test]
    fn handle_none_is_all_ones() {
        assert_eq!(Handle::NONE.0, u32::MAX);
    }
}
