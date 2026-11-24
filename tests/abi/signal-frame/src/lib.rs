// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Canonical FreeBSD amd64 and aarch64 signal-frame model pinned by S160 / CONTRACTS C19.
//!
//! The donor definitions are FreeBSD `sys/x86/include/sigframe.h`,
//! `sys/x86/include/ucontext.h`, `sys/sys/_ucontext.h`, `sys/sys/_sigset.h`,
//! `sys/sys/_sigaltstack.h`, `sys/sys/signal.h`, NetBSD
//! `sys/arch/amd64/include/mcontext.h` + `frame_regs.h`, and NetBSD
//! `sys/arch/arm/include/mcontext.h` (`__gregs[35]`, `__fregs`, `__spare[8]`) for the
//! aarch64 half inside the C19/S160 ucontext order. The checked-in
//! `donor-layout.txt` is produced by `donor-layout.c`; `lib_ut.rs` compares every
//! row to these Rust definitions.

#![forbid(unsafe_code)]

use core::ffi::c_void;

/// FreeBSD/NetBSD 64-bit general register.
pub type Register = u64;

/// FreeBSD `__sigset_t`: `_SIG_WORDS` 4 x 32-bit words, 128 signals.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sigset {
    /// Signal bits.
    pub bits: [u32; 4],
}

/// FreeBSD `struct __stack_t`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    /// Alternate stack base.
    pub ss_sp: *mut c_void,
    /// Alternate stack length.
    pub ss_size: u64,
    /// `SS_ONSTACK` / `SS_DISABLE`.
    pub ss_flags: i32,
}

/// FreeBSD `mc_fpstate[64]`, 16-byte aligned.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FpState {
    /// FXSAVE area words.
    pub words: [u64; 64],
}

/// FreeBSD amd64 `mcontext_t`.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mcontext {
    /// Sigcontext compatibility word.
    pub mc_onstack: u64,
    /// General register.
    pub mc_rdi: u64,
    /// General register.
    pub mc_rsi: u64,
    /// General register.
    pub mc_rdx: u64,
    /// General register.
    pub mc_rcx: u64,
    /// General register.
    pub mc_r8: u64,
    /// General register.
    pub mc_r9: u64,
    /// General register.
    pub mc_rax: u64,
    /// General register.
    pub mc_rbx: u64,
    /// General register.
    pub mc_rbp: u64,
    /// General register.
    pub mc_r10: u64,
    /// General register.
    pub mc_r11: u64,
    /// General register.
    pub mc_r12: u64,
    /// General register.
    pub mc_r13: u64,
    /// General register.
    pub mc_r14: u64,
    /// General register.
    pub mc_r15: u64,
    /// Trap number.
    pub mc_trapno: u32,
    /// Segment selector.
    pub mc_fs: u16,
    /// Segment selector.
    pub mc_gs: u16,
    /// Fault address.
    pub mc_addr: u64,
    /// `_MC_*` flags.
    pub mc_flags: u32,
    /// Segment selector.
    pub mc_es: u16,
    /// Segment selector.
    pub mc_ds: u16,
    /// Error word.
    pub mc_err: u64,
    /// Program counter.
    pub mc_rip: u64,
    /// Code selector.
    pub mc_cs: u64,
    /// Flags register.
    pub mc_rflags: u64,
    /// Stack pointer.
    pub mc_rsp: u64,
    /// Stack selector.
    pub mc_ss: u64,
    /// `sizeof(mcontext_t)`.
    pub mc_len: i64,
    /// FP format.
    pub mc_fpformat: i64,
    /// FP ownership.
    pub mc_ownedfp: i64,
    /// FXSAVE state.
    pub mc_fpstate: FpState,
    /// FS base.
    pub mc_fsbase: u64,
    /// GS base.
    pub mc_gsbase: u64,
    /// Extended XSAVE state pointer.
    pub mc_xfpustate: u64,
    /// Extended XSAVE state length.
    pub mc_xfpustate_len: u64,
    /// TLS base.
    pub mc_tlsbase: u64,
    /// Spare words.
    pub mc_spare: [i64; 3],
}

/// FreeBSD amd64 `ucontext_t`.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ucontext {
    /// Blocked signal mask.
    pub uc_sigmask: Sigset,
    /// Machine context.
    pub uc_mcontext: Mcontext,
    /// Continuation context.
    pub uc_link: *mut Ucontext,
    /// Alternate stack.
    pub uc_stack: Stack,
    /// Context flags.
    pub uc_flags: i32,
    /// Reserved.
    pub __spare__: [i32; 4],
}

/// `union sigval`.
#[repr(C)]
#[derive(Clone, Copy)]
pub union Sigval {
    /// Integer value.
    pub sival_int: i32,
    /// Pointer value.
    pub sival_ptr: *mut c_void,
}

/// `SIGILL`/`SIGFPE` machine-specific reason.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Fault {
    /// Trap number.
    pub _trapno: i32,
}

/// Timer reason.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Timer {
    /// Timer id.
    pub _timerid: i32,
    /// Overrun count.
    pub _overrun: i32,
}

/// Message-queue reason.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Mesgq {
    /// Message queue id.
    pub _mqd: i32,
}

/// Poll reason.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Poll {
    /// Band event.
    pub _band: i64,
}

/// Capsicum reason.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Capsicum {
    /// Syscall number.
    pub _syscall: i32,
}

/// Spare reason, the largest union member.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Spare {
    /// Spare word.
    pub __spare1__: i64,
    /// Spare words.
    pub __spare2__: [i32; 7],
}

/// FreeBSD `siginfo_t` reason union.
#[repr(C)]
#[derive(Clone, Copy)]
pub union Reason {
    /// Machine fault.
    pub fault: Fault,
    /// Timer.
    pub timer: Timer,
    /// Message queue.
    pub mesgq: Mesgq,
    /// Poll.
    pub poll: Poll,
    /// Capsicum.
    pub capsicum: Capsicum,
    /// Spare.
    pub spare: Spare,
}

/// FreeBSD `siginfo_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Siginfo {
    /// Signal number.
    pub si_signo: i32,
    /// Errno association.
    pub si_errno: i32,
    /// Signal code.
    pub si_code: i32,
    /// Sender pid.
    pub si_pid: i32,
    /// Sender uid.
    pub si_uid: u32,
    /// Exit status.
    pub si_status: i32,
    /// Faulting instruction or address.
    pub si_addr: *mut c_void,
    /// Signal value.
    pub si_value: Sigval,
    /// Signal-specific reason.
    pub _reason: Reason,
}

/// FreeBSD amd64 `struct sigframe`.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct Sigframe {
    /// Handler/action union, one pointer.
    pub sf_ahu: *mut c_void,
    /// Embedded ucontext.
    pub sf_uc: Ucontext,
    /// Embedded siginfo.
    pub sf_si: Siginfo,
}

/// NetBSD `__fpregset_t`; the FXSAVE bytes and the XSAVE overlay are 512 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Xsave {
    /// FXSAVE part.
    pub fxsave: [u8; 416],
    /// Reserved.
    pub rsvd: [u8; 48],
    /// XSAVE pointer.
    pub xsaveptr: u64,
    /// XSAVE length.
    pub xsavelen: u64,
    /// Padding.
    pub pad: [u8; 32],
}

/// NetBSD `__fpregset_t` union.
#[repr(C)]
#[derive(Clone, Copy)]
pub union Fpregset {
    /// Raw FXSAVE/XSAVE area.
    pub fxsave: [u8; 512],
    /// XSAVE overlay.
    pub xsave: Xsave,
}

/// NetBSD amd64 `mcontext_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NbMcontext {
    /// General registers, `_NGREG` 26.
    pub __gregs: [u64; 26],
    /// TLS base.
    pub _mc_tlsbase: u64,
    /// FP state.
    pub __fpregs: Fpregset,
}

/// NetBSD `_REG_RDI`.
pub const NB_GREG_RDI: usize = 0;
/// NetBSD `_REG_RSI`.
pub const NB_GREG_RSI: usize = 1;
/// NetBSD `_REG_RDX`.
pub const NB_GREG_RDX: usize = 2;
/// NetBSD `_REG_RCX`.
pub const NB_GREG_RCX: usize = 3;
/// NetBSD `_REG_R8`.
pub const NB_GREG_R8: usize = 4;
/// NetBSD `_REG_R9`.
pub const NB_GREG_R9: usize = 5;
/// NetBSD `_REG_R10`.
pub const NB_GREG_R10: usize = 6;
/// NetBSD `_REG_R11`.
pub const NB_GREG_R11: usize = 7;
/// NetBSD `_REG_R12`.
pub const NB_GREG_R12: usize = 8;
/// NetBSD `_REG_R13`.
pub const NB_GREG_R13: usize = 9;
/// NetBSD `_REG_R14`.
pub const NB_GREG_R14: usize = 10;
/// NetBSD `_REG_R15`.
pub const NB_GREG_R15: usize = 11;
/// NetBSD `_REG_RBP`.
pub const NB_GREG_RBP: usize = 12;
/// NetBSD `_REG_RBX`.
pub const NB_GREG_RBX: usize = 13;
/// NetBSD `_REG_RAX`.
pub const NB_GREG_RAX: usize = 14;
/// NetBSD `_REG_GS`.
pub const NB_GREG_GS: usize = 15;
/// NetBSD `_REG_FS`.
pub const NB_GREG_FS: usize = 16;
/// NetBSD `_REG_ES`.
pub const NB_GREG_ES: usize = 17;
/// NetBSD `_REG_DS`.
pub const NB_GREG_DS: usize = 18;
/// NetBSD `_REG_TRAPNO`.
pub const NB_GREG_TRAPNO: usize = 19;
/// NetBSD `_REG_ERR`.
pub const NB_GREG_ERR: usize = 20;
/// NetBSD `_REG_RIP`.
pub const NB_GREG_RIP: usize = 21;
/// NetBSD `_REG_CS`.
pub const NB_GREG_CS: usize = 22;
/// NetBSD `_REG_RFLAGS`.
pub const NB_GREG_RFLAGS: usize = 23;
/// NetBSD `_REG_RSP`.
pub const NB_GREG_RSP: usize = 24;
/// NetBSD `_REG_SS`.
pub const NB_GREG_SS: usize = 25;

/// aarch64 `__fregset_t`: 32 128-bit SIMD registers, FPCR and FPSR.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aarch64Fregset {
    /// `__qregs[32]`, 16 bytes each.
    pub qregs: [[u8; 16]; 32],
    /// Floating-point control register.
    pub fpcr: u32,
    /// Floating-point status register.
    pub fpsr: u32,
}

/// aarch64 `mcontext_t`: `__gregs`, `__fregs`, `__spare[8]`.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aarch64Mcontext {
    /// `__gregs`: x0-x30, SP, ELR, SPSR, TPIDR.
    pub __gregs: [u64; 35],
    /// `__fregs`.
    pub __fregs: Aarch64Fregset,
    /// `__spare[8]`.
    pub __spare: [u64; 8],
}

/// aarch64 `ucontext_t` in the C19/S160 order.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aarch64Ucontext {
    /// Blocked signal mask.
    pub uc_sigmask: Sigset,
    /// Machine context.
    pub uc_mcontext: Aarch64Mcontext,
    /// Continuation context.
    pub uc_link: *mut Aarch64Ucontext,
    /// Alternate stack.
    pub uc_stack: Stack,
    /// Context flags.
    pub uc_flags: i32,
    /// Reserved.
    pub __spare__: [i32; 4],
}

/// aarch64 `_NGREG`.
pub const AARCH64_NGREG: usize = 35;
/// aarch64 `_REG_SP`.
pub const AARCH64_REG_SP: usize = 31;
/// aarch64 `_REG_ELR`.
pub const AARCH64_REG_ELR: usize = 32;
/// aarch64 `_REG_SPSR`.
pub const AARCH64_REG_SPSR: usize = 33;
/// aarch64 `_REG_TPIDR`.
pub const AARCH64_REG_TPIDR: usize = 34;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
