// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2004-2005 David Schultz <das@FreeBSD.ORG>
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/msun/amd64/fenv.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: none)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! x86_64 `fenv`: the x87 control/status words and `mxcsr`, kept consistent.

use core::arch::asm;

use crate::consts::{FE_ALL_EXCEPT, FE_DOWNWARD, FE_TONEAREST, FE_TOWARDZERO, FE_UPWARD};

const INITIAL_FPUCW: u16 = 0x037f;
const INITIAL_MXCSR: u32 = 0x1f80;
/// Shift between the x87 and SSE exception-mask bits (`_SSE_EMASK_SHIFT`).
const SSE_EMASK_SHIFT: u32 = 7;
const X87_RC_MASK: u16 = 0x0c00;
const MXCSR_RC_MASK: u32 = 0x6000;

/// The C `fenv_t`: 28 bytes of x87 environment plus `mxcsr` (32 bytes, 16-byte aligned).
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FenvT {
    /// The `fnstenv` image.
    pub x87: [u8; 28],
    /// The SSE control/status word.
    pub mxcsr: u32,
}

/// The default environment (`FE_DFL_ENV`).
pub const FE_DFL_ENV: FenvT = FenvT {
    x87: [
        0x7f, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
        0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ],
    mxcsr: INITIAL_MXCSR,
};

/// `fexcept_t`: the exception flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FexceptT(pub u32);

/// `femode_t`: the control state without the flags.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Femode {
    /// The x87 control word.
    pub control: u16,
    /// Reserved padding, zero.
    pub reserved: u16,
    /// The SSE control/status word.
    pub mxcsr: u32,
}

impl Default for Femode {
    fn default() -> Self {
        Self {
            control: INITIAL_FPUCW,
            reserved: 0,
            mxcsr: INITIAL_MXCSR,
        }
    }
}

fn word(x87: &[u8; 28], offset: usize) -> u16 {
    let low = x87.get(offset).copied().unwrap_or(0);
    let high = x87.get(offset + 1).copied().unwrap_or(0);
    u16::from_le_bytes([low, high])
}

fn put_word(x87: &mut [u8; 28], offset: usize, value: u16) {
    let bytes = value.to_le_bytes();
    if let Some(slot) = x87.get_mut(offset..offset + 2) {
        slot.copy_from_slice(&bytes);
    }
}

unsafe fn fnstenv(env: &mut [u8; 28]) {
    // SAFETY: `env` is a valid 28-byte stack/struct buffer for `fnstenv`.
    unsafe {
        asm!("fnstenv [{p}]", p = in(reg) env.as_mut_ptr(), options(nostack, preserves_flags));
    }
}

unsafe fn fldenv(env: &[u8; 28]) {
    // SAFETY: `env` is a valid 28-byte `fnstenv` image.
    unsafe {
        asm!("fldenv [{p}]", p = in(reg) env.as_ptr(), options(nostack, preserves_flags));
    }
}

unsafe fn fnstcw(out: &mut u16) {
    // SAFETY: `out` is a valid two-byte destination for `fnstcw`.
    unsafe {
        asm!("fnstcw [{p}]", p = in(reg) out as *mut u16, options(nostack, preserves_flags));
    }
}

unsafe fn fldcw(value: &u16) {
    // SAFETY: `value` is a valid two-byte control word.
    unsafe {
        asm!("fldcw [{p}]", p = in(reg) value as *const u16, options(nostack, preserves_flags));
    }
}

unsafe fn fnstsw(out: &mut u16) {
    // SAFETY: `out` is a valid two-byte destination for `fnstsw`.
    unsafe {
        asm!("fnstsw [{p}]", p = in(reg) out as *mut u16, options(nostack, preserves_flags));
    }
}

unsafe fn stmxcsr(out: &mut u32) {
    // SAFETY: `out` is a valid four-byte destination; the target has SSE2.
    unsafe {
        asm!("stmxcsr [{p}]", p = in(reg) out as *mut u32, options(nostack, preserves_flags));
    }
}

unsafe fn ldmxcsr(value: &u32) {
    // SAFETY: `value` is a valid four-byte `mxcsr` image; the target has SSE2.
    unsafe {
        asm!("ldmxcsr [{p}]", p = in(reg) value as *const u32, options(nostack, preserves_flags));
    }
}

unsafe fn fnclex() {
    // SAFETY: `fnclex` only touches the x87 status word.
    unsafe {
        asm!("fnclex", options(nostack, preserves_flags));
    }
}

unsafe fn fwait() {
    // SAFETY: `fwait` waits for pending x87 exceptions.
    unsafe {
        asm!("fwait", options(nostack, preserves_flags));
    }
}

fn current_flags() -> u32 {
    let mut status = 0u16;
    let mut mxcsr = 0u32;
    // SAFETY: both destinations are local and valid.
    unsafe {
        fnstsw(&mut status);
        stmxcsr(&mut mxcsr);
    }
    (u32::from(status) | mxcsr) & FE_ALL_EXCEPT
}

/// Clear the given exception flags in both the x87 status word and `mxcsr`.
pub fn feclearexcept(excepts: i32) -> i32 {
    let excepts = (excepts as u32) & FE_ALL_EXCEPT;
    let mut env = [0u8; 28];
    let mut mxcsr = 0u32;
    // SAFETY: the buffers are local and valid.
    unsafe {
        fnstenv(&mut env);
        stmxcsr(&mut mxcsr);
        let status = word(&env, 4);
        put_word(&mut env, 4, status & !(excepts as u16));
        fldenv(&env);
        ldmxcsr(&(mxcsr & !excepts));
    }
    0
}

/// Read the given exception flags.
pub fn fegetexceptflag(flagp: &mut FexceptT, excepts: i32) -> i32 {
    flagp.0 = current_flags() & (excepts as u32) & FE_ALL_EXCEPT;
    0
}

/// Replace the given exception flags with the saved ones.
pub fn fesetexceptflag(flagp: &FexceptT, excepts: i32) -> i32 {
    let mask = (excepts as u32) & FE_ALL_EXCEPT;
    let mut env = [0u8; 28];
    let mut mxcsr = 0u32;
    // SAFETY: the buffers are local and valid.
    unsafe {
        fnstenv(&mut env);
        let status = word(&env, 4);
        put_word(
            &mut env,
            4,
            (status & !(mask as u16)) | ((flagp.0 & mask) as u16),
        );
        fldenv(&env);
        stmxcsr(&mut mxcsr);
        ldmxcsr(&((mxcsr & !mask) | (flagp.0 & mask)));
    }
    0
}

/// Raise the given exception flags.
pub fn feraiseexcept(excepts: i32) -> i32 {
    let saved = FexceptT((excepts as u32) & FE_ALL_EXCEPT);
    fesetexceptflag(&saved, excepts);
    // SAFETY: `fwait` only synchronises the x87 unit.
    unsafe {
        fwait();
    }
    0
}

/// Test the given exception flags.
pub fn fetestexcept(excepts: i32) -> i32 {
    (current_flags() & (excepts as u32) & FE_ALL_EXCEPT) as i32
}

/// The current rounding mode.
pub fn fegetround() -> i32 {
    let mut control = 0u16;
    // SAFETY: `control` is a local valid destination.
    unsafe {
        fnstcw(&mut control);
    }
    i32::from(control & X87_RC_MASK)
}

/// Set the rounding mode in both the x87 control word and `mxcsr`.
pub fn fesetround(round: i32) -> i32 {
    let value = round as u32;
    if !matches!(
        value,
        FE_TONEAREST | FE_DOWNWARD | FE_UPWARD | FE_TOWARDZERO
    ) {
        return -1;
    }
    let mut control = 0u16;
    let mut mxcsr = 0u32;
    // SAFETY: the buffers are local and valid.
    unsafe {
        fnstcw(&mut control);
        stmxcsr(&mut mxcsr);
        control = (control & !X87_RC_MASK) | (value as u16);
        fldcw(&control);
        ldmxcsr(&((mxcsr & !MXCSR_RC_MASK) | (value << 3)));
    }
    0
}

/// Save the environment.
pub fn fegetenv(envp: &mut FenvT) -> i32 {
    // SAFETY: `envp` is a valid `FenvT` (28-byte image + mxcsr).
    unsafe {
        fnstenv(&mut envp.x87);
        stmxcsr(&mut envp.mxcsr);
        // `fnstenv` masks all exceptions; restore the saved control word.
        fldcw(&word(&envp.x87, 0));
    }
    0
}

/// Save the environment and mask all exceptions (non-stop mode).
pub fn feholdexcept(envp: &mut FenvT) -> i32 {
    let mut mxcsr = 0u32;
    // SAFETY: the buffers are local/valid.
    unsafe {
        stmxcsr(&mut mxcsr);
        fnstenv(&mut envp.x87);
        fnclex();
        envp.mxcsr = mxcsr;
        let masked = (mxcsr & !FE_ALL_EXCEPT) | (FE_ALL_EXCEPT << SSE_EMASK_SHIFT);
        ldmxcsr(&masked);
    }
    0
}

/// Restore the environment.
pub fn fesetenv(envp: &FenvT) -> i32 {
    // SAFETY: `envp` is a valid saved environment.
    unsafe {
        fldenv(&envp.x87);
        ldmxcsr(&envp.mxcsr);
    }
    0
}

/// Restore the default environment.
pub fn fesetenv_default() -> i32 {
    fesetenv(&FE_DFL_ENV)
}

/// Restore the environment, keeping the flags raised since it was saved.
pub fn feupdateenv(envp: &FenvT) -> i32 {
    let raised = current_flags();
    fesetenv(envp);
    feraiseexcept(raised as i32)
}

/// Enable traps for the given exceptions; returns the previous mask.
pub fn feenableexcept(mask: i32) -> i32 {
    let mask = (mask as u32) & FE_ALL_EXCEPT;
    let mut control = 0u16;
    let mut mxcsr = 0u32;
    // SAFETY: the buffers are local and valid.
    unsafe {
        fnstcw(&mut control);
        stmxcsr(&mut mxcsr);
        let previous = !(u32::from(control) | (mxcsr >> SSE_EMASK_SHIFT)) & FE_ALL_EXCEPT;
        control &= !(mask as u16);
        fldcw(&control);
        ldmxcsr(&(mxcsr & !(mask << SSE_EMASK_SHIFT)));
        previous as i32
    }
}

/// Disable traps for the given exceptions; returns the previous mask.
pub fn fedisableexcept(mask: i32) -> i32 {
    let mask = (mask as u32) & FE_ALL_EXCEPT;
    let mut control = 0u16;
    let mut mxcsr = 0u32;
    // SAFETY: the buffers are local and valid.
    unsafe {
        fnstcw(&mut control);
        stmxcsr(&mut mxcsr);
        let previous = !(u32::from(control) | (mxcsr >> SSE_EMASK_SHIFT)) & FE_ALL_EXCEPT;
        control |= mask as u16;
        fldcw(&control);
        ldmxcsr(&(mxcsr | (mask << SSE_EMASK_SHIFT)));
        previous as i32
    }
}

/// The currently enabled trap mask.
pub fn fegetexcept() -> i32 {
    let mut control = 0u16;
    let mut mxcsr = 0u32;
    // SAFETY: the buffers are local and valid.
    unsafe {
        fnstcw(&mut control);
        stmxcsr(&mut mxcsr);
    }
    (!(u32::from(control) | (mxcsr >> SSE_EMASK_SHIFT)) & FE_ALL_EXCEPT) as i32
}

/// Save the control state (C23 `fegetmode`).
pub fn fegetmode(modep: &mut Femode) -> i32 {
    let mut control = 0u16;
    let mut mxcsr = 0u32;
    // SAFETY: the buffers are local and valid.
    unsafe {
        fnstcw(&mut control);
        stmxcsr(&mut mxcsr);
    }
    modep.control = control;
    modep.mxcsr = mxcsr;
    0
}

/// Restore the control state (C23 `fesetmode`).
pub fn fesetmode(modep: &Femode) -> i32 {
    // SAFETY: the values are valid control words.
    unsafe {
        fldcw(&modep.control);
        ldmxcsr(&modep.mxcsr);
    }
    0
}

#[cfg(test)]
#[path = "x86_64_ut.rs"]
mod tests;
