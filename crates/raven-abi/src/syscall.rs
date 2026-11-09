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

    /// `thread_create(proc: Handle, entry: usize, stack_top: usize, arg: usize, flags: usize) -> Handle`
    /// creates a thread in `proc` starting at `entry` with the stack pointer at `stack_top`;
    /// `arg` is passed in the first argument register (`rdi`/`x0`/`a0`).
    THREAD_CREATE = 0x20;
    /// `thread_exit(code: i32) -> !` terminates the calling thread.
    THREAD_EXIT = 0x21;

    /// `proc_create(flags: usize) -> Handle` creates an empty process (no threads) and returns a
    /// handle with all rights.
    PROC_CREATE = 0x30;

    /// `map(proc: Handle, memobj: Handle, offset: usize, addr: usize, len: usize, flags: usize) -> addr`
    /// maps `len` bytes (page multiple) of `memobj` (or fresh zeroed memory when `memobj ==
    /// Handle::NONE`) at `addr` (hint, or exact with `MAP_FIXED`) in `proc`. `flags` encodes
    /// `prot | (map_flags << 8)`, see [`crate::types::prot`] and [`crate::types::map`].
    MAP = 0x40;
    /// `unmap(proc: Handle, addr: usize, len: usize) -> 0`.
    UNMAP = 0x41;
    /// `protect(proc: Handle, addr: usize, len: usize, prot: usize) -> 0`.
    PROTECT = 0x42;

    /// `handle_close(h: Handle) -> 0`.
    HANDLE_CLOSE = 0x50;
    /// `handle_dup(h: Handle, rights_mask: u32) -> Handle`; new rights = old rights AND mask. Requires `DUP`.
    HANDLE_DUP = 0x51;
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
        assert_eq!(Syscall::PROC_CREATE as usize, 0x30);
        assert_eq!(Syscall::MAP as usize, 0x40);
        assert_eq!(Syscall::HANDLE_DUP as usize, 0x51);
    }

    #[test]
    fn handle_none_is_all_ones() {
        assert_eq!(Handle::NONE.0, u32::MAX);
    }
}
