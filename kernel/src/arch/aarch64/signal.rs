// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 signal-frame layout (CONTRACTS C19).
//!
//! The kernel-owned `sendsig` frame for aarch64: the NetBSD `sendsig_siginfo` placement
//! (`sys/arch/aarch64/aarch64/sig_machdep.c`), the NetBSD aarch64 `mcontext_t`
//! (`sys/arch/arm/include/mcontext.h`: `__gregs[35]`, `__fregs`, `__spare[8]`), and the C19/S160
//! `ucontext_t` field order with the aarch64 machine context. The aarch64 vectors call
//! [`frame`] and install [`SIGCODE`] at exec; the generic S44 syscall path owns the handler
//! table and the copyout.
//!
//! All values are bytes. The host test in `tests/conformance/aarch64/signal-frame` and the
//! const assertions below pin the layout against the S160 table and the NetBSD donor.

use core::mem::{align_of, offset_of, size_of};

use raven_abi::signal::SIGRETURN;

/// The aarch64 sigcode trampoline words: `mov x8, #SIGRETURN; svc #0`.
pub const SIGCODE_WORDS: [u32; 2] = [
    0xD280_0000 | ((SIGRETURN as u32) << 5) | 8, // movz x8, #0x82
    0xD400_0001,                                 // svc #0
];

/// The little-endian sigcode bytes the kernel installs at exec.
pub const SIGCODE: [u8; 8] = {
    let first = SIGCODE_WORDS[0].to_le_bytes();
    let second = SIGCODE_WORDS[1].to_le_bytes();
    [
        first[0], first[1], first[2], first[3], second[0], second[1], second[2], second[3],
    ]
};

/// `__gregs` index of `x0`.
pub const REG_X0: usize = 0;
/// `__gregs` index of `x28`.
pub const REG_X28: usize = 28;
/// `__gregs` index of `x29` (frame pointer).
pub const REG_X29: usize = 29;
/// `__gregs` index of `x30` (link register).
pub const REG_X30: usize = 30;
/// `__gregs` index of the saved stack pointer (`_REG_SP` is `_REG_X31` on NetBSD).
pub const REG_SP: usize = 31;
/// `__gregs` index of `ELR`.
pub const REG_ELR: usize = 32;
/// `__gregs` index of `SPSR`.
pub const REG_SPSR: usize = 33;
/// `__gregs` index of `TPIDR`.
pub const REG_TPIDR: usize = 34;
/// Number of `__gregs` slots (NetBSD `_NGREG`).
pub const NGREG: usize = 35;
/// Number of `__fregs` SIMD slots (NetBSD `_NFREG`).
pub const NFREG: usize = 32;

/// `sendsig_siginfo` handler argument registers: `x0=sig`, `x1=&si`, `x2=&uc`, `x28=&uc`.
pub const ARG_SIG: usize = REG_X0;
/// `sendsig_siginfo` siginfo argument register.
pub const ARG_SIGINFO: usize = 1;
/// `sendsig_siginfo` ucontext argument register.
pub const ARG_UCONTEXT: usize = 2;
/// `sendsig_siginfo` callee-saved copy of the ucontext pointer.
pub const ARG_UCONTEXT_CALLEE: usize = REG_X28;
/// The handler's return address register is set to the sigcode address.
pub const REG_LR: usize = REG_X30;

/// `__fregs` (`__fregset_t`): 32 128-bit SIMD registers, FPCR and FPSR.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FpregsetLayout {
    /// `__qregs[32]`, 16 bytes each.
    pub q: [[u8; 16]; NFREG],
    /// `FPCR`.
    pub fpcr: u32,
    /// `FPSR`.
    pub fpsr: u32,
}

/// `mcontext_t`: `__gregs[35]`, `__fregs`, `__spare[8]` (NetBSD aarch64).
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MContextLayout {
    /// `__gregs`: x0-x30, SP, ELR, SPSR, TPIDR.
    pub gregs: [u64; NGREG],
    /// `__fregs`: the SIMD/FP register file.
    pub fregs: FpregsetLayout,
    /// `__spare[8]`.
    pub spare: [u64; 8],
}

/// `stack_t`: `ss_sp`, `ss_size`, `ss_flags`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackLayout {
    /// Alternate-stack base.
    pub ss_sp: u64,
    /// Alternate-stack size.
    pub ss_size: u64,
    /// `SS_ONSTACK`/`SS_DISABLE`.
    pub ss_flags: u32,
    /// ABI padding.
    pub spare: u32,
}

/// `ucontext_t` with the C19/S160 field order and the aarch64 machine context.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UContextLayout {
    /// Signal mask, first as in the S160/FreeBSD layout.
    pub uc_sigmask: [u32; 4],
    /// Machine context (the NetBSD aarch64 layout above).
    pub uc_mcontext: MContextLayout,
    /// Context to resume.
    pub uc_link: u64,
    /// Alternate stack.
    pub uc_stack: StackLayout,
    /// `_UC_*` flags.
    pub uc_flags: u32,
    /// `__spare__[4]`.
    pub uc_spare: [u32; 4],
}

/// `sizeof(mcontext_t)`.
pub const MCONTEXT_SIZE: usize = size_of::<MContextLayout>();
/// `_Alignof(mcontext_t)`.
pub const MCONTEXT_ALIGN: usize = align_of::<MContextLayout>();
/// `offsetof(mcontext_t, __gregs)`.
pub const MC_GREGS_OFFSET: usize = offset_of!(MContextLayout, gregs);
/// `offsetof(mcontext_t, __fregs)`.
pub const MC_FREGS_OFFSET: usize = offset_of!(MContextLayout, fregs);
/// `offsetof(mcontext_t, __spare)`.
pub const MC_SPARE_OFFSET: usize = offset_of!(MContextLayout, spare);
/// `sizeof(__fregset_t)`.
pub const FREGSET_SIZE: usize = size_of::<FpregsetLayout>();
/// `sizeof(ucontext_t)`.
pub const UCONTEXT_SIZE: usize = size_of::<UContextLayout>();
/// `_Alignof(ucontext_t)`.
pub const UCONTEXT_ALIGN: usize = align_of::<UContextLayout>();
/// `offsetof(ucontext_t, uc_sigmask)`.
pub const UC_SIGMASK_OFFSET: usize = offset_of!(UContextLayout, uc_sigmask);
/// `offsetof(ucontext_t, uc_mcontext)`.
pub const UC_MCONTEXT_OFFSET: usize = offset_of!(UContextLayout, uc_mcontext);
/// `offsetof(ucontext_t, uc_link)`.
pub const UC_LINK_OFFSET: usize = offset_of!(UContextLayout, uc_link);
/// `offsetof(ucontext_t, uc_stack)`.
pub const UC_STACK_OFFSET: usize = offset_of!(UContextLayout, uc_stack);
/// `offsetof(ucontext_t, uc_flags)`.
pub const UC_FLAGS_OFFSET: usize = offset_of!(UContextLayout, uc_flags);
/// `sizeof(siginfo_t)` from the C19/S160 table.
pub const SIGINFO_SIZE: usize = 0x50;
/// `_Alignof(siginfo_t)` from the C19/S160 table.
pub const SIGINFO_ALIGN: usize = 8;
/// Signal frames are 16-byte aligned on the user stack.
pub const FRAME_ALIGN: u64 = 16;

// The aarch64 kernel build checks these before the host test exists in the image.
const _: () = {
    assert!(MCONTEXT_SIZE == 0x370);
    assert!(MCONTEXT_ALIGN == 16);
    assert!(MC_GREGS_OFFSET == 0);
    assert!(MC_FREGS_OFFSET == 0x120);
    assert!(MC_SPARE_OFFSET == 0x330);
    assert!(FREGSET_SIZE == 0x210);
    assert!(UCONTEXT_SIZE == 0x3c0);
    assert!(UCONTEXT_ALIGN == 16);
    assert!(UC_SIGMASK_OFFSET == 0);
    assert!(UC_MCONTEXT_OFFSET == 0x10);
    assert!(UC_LINK_OFFSET == 0x380);
    assert!(UC_STACK_OFFSET == 0x388);
    assert!(UC_FLAGS_OFFSET == 0x3a0);
    assert!(SIGINFO_SIZE == 0x50);
    assert!(SIGINFO_ALIGN == 8);
    assert!(SIGCODE_WORDS[0] == 0xD280_1048);
    assert!(SIGCODE_WORDS[1] == 0xD400_0001);
};

/// The aarch64 `sendsig` frame: `siginfo_t` below `ucontext_t`, both below the aligned
/// interrupted stack pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SigFrame {
    /// The final `tf_sp` (the `siginfo` address).
    pub sp: u64,
    /// The `siginfo_t` address (`x1`).
    pub siginfo: u64,
    /// The `ucontext_t` address (`x2` and `x28`).
    pub ucontext: u64,
}

/// Round `value` up to `align`, a power of two.
pub const fn round_up(value: usize, align: usize) -> usize {
    let mask = align.wrapping_sub(1);
    value.wrapping_add(mask) & !mask
}

/// Compute the NetBSD `sendsig_siginfo` placement: align the interrupted `sp`, put the
/// `ucontext_t` below it, then the `siginfo_t` below that. `None` on underflow.
pub const fn frame(interrupted_sp: u64) -> Option<SigFrame> {
    let top = interrupted_sp & !(FRAME_ALIGN - 1);
    let ucontext = match top.checked_sub(UCONTEXT_SIZE as u64) {
        Some(value) => value,
        None => return None,
    };
    let siginfo = match ucontext.checked_sub(round_up(SIGINFO_SIZE, FRAME_ALIGN as usize) as u64) {
        Some(value) => value,
        None => return None,
    };
    Some(SigFrame {
        sp: siginfo,
        siginfo,
        ucontext,
    })
}

#[cfg(test)]
#[path = "sigframe_ut.rs"]
mod sigframe_tests;
#[cfg(test)]
#[path = "signal_ut.rs"]
mod tests;
