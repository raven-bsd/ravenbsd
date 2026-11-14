// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The strong `.fini_array` runner (`donor-map/16` 3.A.4.3, hook `fini_array`).
//!
//! `__raven_libc_start` registers this function with `__cxa_atexit` as the process finaliser
//! (step 13 of `donor-map/10` 3.12.2). It runs the `.fini_array` entries in reverse order —
//! constructors ran forwards, destructors run backwards — then `_fini()` when the toolchain
//! still emits one. Entries `0`, `1` and `-1` are placeholders and are skipped (`A-ld-20`).
//!
//! The linker symbols are declared weak: an executable without the section has no array to
//! run, not a link error.

core::arch::global_asm!(
    ".weak __fini_array_start",
    ".weak __fini_array_end",
    ".weak _fini",
);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".text",
    ".globl __rl_fini_hook",
    ".type __rl_fini_hook,@function",
    "__rl_fini_hook:",
    "mov _fini@GOTPCREL(%rip),%rax",
    "ret",
    options(att_syntax),
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".text",
    ".globl __rl_fini_hook",
    ".type __rl_fini_hook,%function",
    "__rl_fini_hook:",
    "adrp x0, :got:_fini",
    "ldr x0, [x0, :got_lo12:_fini]",
    "ret",
);

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    ".text",
    ".globl __rl_fini_hook",
    ".type __rl_fini_hook,%function",
    "__rl_fini_hook:",
    "1:",
    "auipc a0, %got_pcrel_hi(_fini)",
    "ld a0, %pcrel_lo(1b)(a0)",
    "ret",
);

unsafe extern "C" {
    /// First `.fini_array` entry.
    static __fini_array_start: usize;
    /// One past the last `.fini_array` entry.
    static __fini_array_end: usize;
    /// The legacy destructor (a weak no-op is absent in the modern toolchain).
    fn _fini();
    /// The runtime address of the weak `_fini`, or 0 when absent.
    fn __rl_fini_hook() -> usize;
}

/// Run `.fini_array` backwards and then `_fini()`.
///
/// The `__cxa_atexit` argument is unused; it exists so the address has the callback shape.
#[unsafe(no_mangle)]
pub extern "C" fn __rl_fini_array(_argument: *mut core::ffi::c_void) {
    let start = core::ptr::addr_of!(__fini_array_start);
    let end = core::ptr::addr_of!(__fini_array_end);
    let mut cursor = end;
    while cursor > start {
        cursor = cursor.wrapping_sub(1);
        // SAFETY: `cursor` is inside the linker-bounded `.fini_array` section.
        let entry = unsafe { *cursor };
        if entry != 0 && entry != 1 && entry != usize::MAX {
            // SAFETY: a non-placeholder entry is a destructor of this executable.
            let destructor: extern "C" fn() = unsafe { core::mem::transmute(entry) };
            destructor();
        }
    }
    // SAFETY: the assembly helper reads the weak `_fini` address without the optimiser's
    // non-null assumption; a zero result means the toolchain emitted no legacy destructor.
    if unsafe { __rl_fini_hook() } != 0 {
        // SAFETY: a non-zero weak `_fini` is the executable's own legacy destructor.
        unsafe {
            _fini();
        }
    }
}
