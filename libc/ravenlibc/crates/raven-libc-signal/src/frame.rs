// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Frame layouts: views over the ABI `ucontext_t`/`mcontext_t` and the `siginfo_t` fields.
//!
//! CLEANROOM. The ABI crate fixes the sizes; this module supplies the typed accessors of ticket
//! section 4.6. The kernel-saved area uses the `mcontext_t` layout (PROPOSED, RD-new 3), so
//! COPY-IN/COPY-OUT is one `memcpy` of `size_of::<McontextT>()` bytes. x86-64 stores `RIP` at
//! `gregs[16]` and `RSP` at `gregs[15]`; AArch64 keeps `pc`/`sp` after `regs[31]`; RISC-V uses
//! `__gregs[0]` (pc), `[1]` (ra) and `[2]` (sp).

use raven_libc_abi::generated::signal;
use raven_libc_abi::structs::{McontextT, SiginfoT, Sigval, StackT, UcontextT};

use crate::kernel::{FaultKind, FaultRecord};

/// Word index of `uc_mcontext` in `ucontext_t`.
#[cfg(target_arch = "x86_64")]
pub const UC_MCONTEXT_WORD: usize = 5;
/// Word index of `uc_sigmask` in `ucontext_t`.
#[cfg(target_arch = "x86_64")]
pub const UC_SIGMASK_WORD: usize = 37;
/// Word index of `uc_mcontext` in `ucontext_t`.
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
pub const UC_MCONTEXT_WORD: usize = 22;
/// Word index of `uc_sigmask` in `ucontext_t`.
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
pub const UC_SIGMASK_WORD: usize = 5;

/// Word index of `uc_stack.ss_sp`.
pub const UC_STACK_WORD: usize = 2;
/// Word index of the x86-64 legacy FP area (`__fpregs_mem`).
#[cfg(target_arch = "x86_64")]
pub const UC_FP_AREA_WORD: usize = 53;

/// `REG_RIP` of the x86-64 `gregset_t`.
#[cfg(target_arch = "x86_64")]
pub const GREG_PC: usize = 16;
/// `REG_RSP` of the x86-64 `gregset_t`.
#[cfg(target_arch = "x86_64")]
pub const GREG_SP: usize = 15;
/// Word index of `pc` in the AArch64 `mcontext_t`.
#[cfg(target_arch = "aarch64")]
pub const GREG_PC: usize = 33;
/// Word index of `sp` in the AArch64 `mcontext_t`.
#[cfg(target_arch = "aarch64")]
pub const GREG_SP: usize = 32;
/// Word index of `regs[30]` (link register) in the AArch64 `mcontext_t`.
#[cfg(target_arch = "aarch64")]
pub const GREG_LR: usize = 31;
/// `__gregs[0]` is `pc` on RISC-V.
#[cfg(target_arch = "riscv64")]
pub const GREG_PC: usize = 0;
/// `__gregs[2]` is `sp` on RISC-V.
#[cfg(target_arch = "riscv64")]
pub const GREG_SP: usize = 2;
/// `__gregs[1]` is `ra` on RISC-V.
#[cfg(target_arch = "riscv64")]
pub const GREG_LR: usize = 1;

/// A raw const pointer to the `mcontext_t` inside a `ucontext_t` (no dereference).
#[must_use]
pub fn uc_mcontext_ptr(context: &UcontextT) -> *const McontextT {
    context
        .bytes
        .as_ptr()
        .wrapping_add(UC_MCONTEXT_WORD)
        .cast::<McontextT>()
}

/// A raw mutable pointer to the `mcontext_t` inside a `ucontext_t` (no dereference).
#[must_use]
pub fn uc_mcontext_ptr_mut(context: &mut UcontextT) -> *mut McontextT {
    context
        .bytes
        .as_mut_ptr()
        .wrapping_add(UC_MCONTEXT_WORD)
        .cast::<McontextT>()
}

/// The interrupted `uc_sigmask` word (only signals 1..64 live in word 0).
#[must_use]
pub fn uc_sigmask_word(context: &UcontextT) -> u64 {
    context.bytes.get(UC_SIGMASK_WORD).copied().unwrap_or(0)
}

/// Store the `uc_sigmask` word.
pub fn set_uc_sigmask_word(context: &mut UcontextT, word: u64) {
    if let Some(slot) = context.bytes.get_mut(UC_SIGMASK_WORD) {
        *slot = word;
    }
}

/// The `uc_stack.ss_sp` of the interrupted context.
#[must_use]
pub fn uc_stack_sp(context: &UcontextT) -> usize {
    context.bytes.get(UC_STACK_WORD).copied().unwrap_or(0) as usize
}

/// Fill `uc_stack` from a `stack_t`.
pub fn set_uc_stack(context: &mut UcontextT, stack: &StackT) {
    if let Some(slot) = context.bytes.get_mut(UC_STACK_WORD) {
        *slot = stack.ss_sp as usize as u64;
    }
    if let Some(slot) = context.bytes.get_mut(UC_STACK_WORD.wrapping_add(1)) {
        *slot = stack.ss_flags as u32 as u64;
    }
    if let Some(slot) = context.bytes.get_mut(UC_STACK_WORD.wrapping_add(2)) {
        *slot = stack.ss_size;
    }
}

/// Read the stored `uc_stack`.
#[must_use]
pub fn uc_stack(context: &UcontextT) -> StackT {
    StackT {
        ss_sp: uc_stack_sp(context) as *mut core::ffi::c_void,
        ss_flags: context
            .bytes
            .get(UC_STACK_WORD.wrapping_add(1))
            .copied()
            .unwrap_or(0) as i32,
        ss_size: context
            .bytes
            .get(UC_STACK_WORD.wrapping_add(2))
            .copied()
            .unwrap_or(0),
    }
}

/// `uc_flags`.
#[must_use]
pub fn uc_flags(context: &UcontextT) -> u64 {
    context.bytes.first().copied().unwrap_or(0)
}

/// `uc_link`.
#[must_use]
pub fn uc_link(context: &UcontextT) -> usize {
    context.bytes.get(1).copied().unwrap_or(0) as usize
}

/// Set `uc_link`.
pub fn set_uc_link(context: &mut UcontextT, link: usize) {
    if let Some(slot) = context.bytes.get_mut(1) {
        *slot = link as u64;
    }
}

/// The program counter of an `mcontext_t`.
#[must_use]
pub fn ctx_pc(context: &McontextT) -> usize {
    context.bytes.get(GREG_PC).copied().unwrap_or(0) as usize
}

/// The stack pointer of an `mcontext_t`.
#[must_use]
pub fn ctx_sp(context: &McontextT) -> usize {
    context.bytes.get(GREG_SP).copied().unwrap_or(0) as usize
}

/// Set the program counter.
pub fn set_ctx_pc(context: &mut McontextT, value: usize) {
    if let Some(slot) = context.bytes.get_mut(GREG_PC) {
        *slot = value as u64;
    }
}

/// Set the stack pointer.
pub fn set_ctx_sp(context: &mut McontextT, value: usize) {
    if let Some(slot) = context.bytes.get_mut(GREG_SP) {
        *slot = value as u64;
    }
}

/// The `lr`/`ra` of the machine context (0 on x86-64, which has none).
#[must_use]
pub fn ctx_lr(context: &McontextT) -> usize {
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        context.bytes.get(GREG_LR).copied().unwrap_or(0) as usize
    }
    #[cfg(target_arch = "x86_64")]
    {
        let _ = context;
        0
    }
}

/// Set the link register on the architectures that have one.
pub fn set_ctx_lr(context: &mut McontextT, value: usize) {
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        if let Some(slot) = context.bytes.get_mut(GREG_LR) {
            *slot = value as u64;
        }
    }
    #[cfg(target_arch = "x86_64")]
    {
        let _ = (context, value);
    }
}

/// Generic register word of an `mcontext_t` (x86-64 `gregs[]`, AArch64/RISC-V `__gregs[]`).
#[must_use]
pub fn ctx_greg(context: &McontextT, index: usize) -> usize {
    context.bytes.get(index).copied().unwrap_or(0) as usize
}

/// Set a generic register word.
pub fn set_ctx_greg(context: &mut McontextT, index: usize, value: usize) {
    if let Some(slot) = context.bytes.get_mut(index) {
        *slot = value as u64;
    }
}

/// `siginfo_t.si_signo`.
#[must_use]
pub fn si_signo(info: &SiginfoT) -> i32 {
    info.si_signo
}

/// `siginfo_t.si_code`.
#[must_use]
pub fn si_code(info: &SiginfoT) -> i32 {
    info.si_code
}

/// A `u64` field of the `siginfo_t` union.
#[must_use]
pub fn si_field(info: &SiginfoT, index: usize) -> u64 {
    info.__si_fields.get(index).copied().unwrap_or(0)
}

/// Set a `u64` field of the union.
pub fn set_si_field(info: &mut SiginfoT, index: usize, value: u64) {
    if let Some(slot) = info.__si_fields.get_mut(index) {
        *slot = value;
    }
}

/// `_kill.si_pid`.
#[must_use]
pub fn si_pid(info: &SiginfoT) -> i32 {
    si_field(info, 0) as u32 as i32
}

/// Set `si_pid`.
pub fn set_si_pid(info: &mut SiginfoT, pid: i32) {
    let word = si_field(info, 0);
    set_si_field(
        info,
        0,
        (word & 0xffff_ffff_0000_0000) | u64::from(pid as u32),
    );
}

/// `_kill.si_uid`.
#[must_use]
pub fn si_uid(info: &SiginfoT) -> u32 {
    (si_field(info, 0) >> 32) as u32
}

/// Set `si_uid`.
pub fn set_si_uid(info: &mut SiginfoT, uid: u32) {
    let word = si_field(info, 0);
    set_si_field(info, 0, (word & 0xffff_ffff) | (u64::from(uid) << 32));
}

/// `_rt.si_value`.
#[must_use]
pub fn si_value(info: &SiginfoT) -> Sigval {
    Sigval {
        bytes: [si_field(info, 1)],
    }
}

/// Set `si_value`.
pub fn set_si_value(info: &mut SiginfoT, value: Sigval) {
    set_si_field(info, 1, value.bytes.first().copied().unwrap_or(0));
}

/// `_sigfault.si_addr`.
#[must_use]
pub fn si_addr(info: &SiginfoT) -> usize {
    si_field(info, 0) as usize
}

/// Set `si_addr`.
pub fn set_si_addr(info: &mut SiginfoT, address: usize) {
    set_si_field(info, 0, address as u64);
}

/// Build a zeroed `siginfo_t` with `si_signo`/`si_code`.
#[must_use]
pub fn make_siginfo(sig: i32, code: i32) -> SiginfoT {
    let mut info = SiginfoT::ZERO;
    info.si_signo = sig;
    info.si_code = code;
    info
}

/// Build a user-space `siginfo_t` (`SI_USER`/`SI_TKILL`).
#[must_use]
pub fn user_siginfo(sig: i32, code: i32) -> SiginfoT {
    let mut info = make_siginfo(sig, code);
    set_si_pid(&mut info, 0);
    set_si_uid(&mut info, 0);
    info
}

/// Build a `SI_QUEUE` record.
#[must_use]
pub fn queue_siginfo(sig: i32, value: Sigval) -> SiginfoT {
    let mut info = make_siginfo(sig, signal::SI_QUEUE);
    set_si_value(&mut info, value);
    info
}

/// Map a fault to `(signal, si_code)`.
#[must_use]
pub fn fault_to_signal(kind: FaultKind) -> (i32, i32) {
    match kind {
        FaultKind::PageNomap => (signal::SIGSEGV, signal::SEGV_MAPERR),
        FaultKind::PagePerm => (signal::SIGSEGV, signal::SEGV_ACCERR),
        FaultKind::Divide => (signal::SIGFPE, signal::FPE_INTDIV),
        FaultKind::Illegal => (signal::SIGILL, signal::ILL_ILLOPC),
        FaultKind::Misalign => (signal::SIGBUS, signal::BUS_ADRALN),
        FaultKind::Breakpoint => (signal::SIGTRAP, signal::TRAP_BRKPT),
    }
}

/// Build the `siginfo_t` of a synchronous fault.
#[must_use]
pub fn fault_siginfo(fault: &FaultRecord) -> Option<SiginfoT> {
    let kind = FaultKind::from_u32(fault.kind)?;
    let (sig, code) = fault_to_signal(kind);
    let mut info = make_siginfo(sig, code);
    match kind {
        FaultKind::Divide | FaultKind::Illegal => set_si_addr(&mut info, fault.pc),
        _ => set_si_addr(&mut info, fault.addr),
    }
    Some(info)
}

/// The `ucontext_t` a handler receives: interrupted mcontext, stack and mask.
pub fn build_ucontext(mcontext: &McontextT, mask: u64, stack: &StackT) -> UcontextT {
    let mut context = UcontextT::ZERO;
    crate::raw::copy_mcontext_in(mcontext, &mut context);
    set_uc_sigmask_word(&mut context, mask);
    set_uc_stack(&mut context, stack);
    context
}

/// Copy a `mcontext_t` out of a `ucontext_t`.
#[must_use]
pub fn mcontext_of(context: &UcontextT) -> McontextT {
    crate::raw::read_mcontext(context)
}

#[cfg(test)]
#[path = "frame_ut.rs"]
mod frame_ut;
