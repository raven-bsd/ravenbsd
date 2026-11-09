// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel trap frame.
//!
//! The assembly stubs push the 15 general-purpose registers in descending order so that the
//! lowest address of the frame is `r15`. `vector` and `error_code` are directly below the GPRs,
//! followed by the CPU frame. The layout is fixed by CONTRACTS/exception reporting and asserted
//! below.

/// Register state at an exception or interrupt entry.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrapFrame {
    /// r15.
    pub r15: u64,
    /// r14.
    pub r14: u64,
    /// r13.
    pub r13: u64,
    /// r12.
    pub r12: u64,
    /// r11.
    pub r11: u64,
    /// r10.
    pub r10: u64,
    /// r9.
    pub r9: u64,
    /// r8.
    pub r8: u64,
    /// rbp.
    pub rbp: u64,
    /// rdi.
    pub rdi: u64,
    /// rsi.
    pub rsi: u64,
    /// rdx.
    pub rdx: u64,
    /// rcx.
    pub rcx: u64,
    /// rbx.
    pub rbx: u64,
    /// rax.
    pub rax: u64,
    /// Exception vector number.
    pub vector: u64,
    /// CPU error code, or zero when the CPU pushes none.
    pub error_code: u64,
    /// Interrupted instruction pointer.
    pub rip: u64,
    /// Interrupted code segment.
    pub cs: u64,
    /// Interrupted flags.
    pub rflags: u64,
    /// Interrupted stack pointer.
    pub rsp: u64,
    /// Interrupted stack segment.
    pub ss: u64,
}

const _: () = assert!(core::mem::size_of::<TrapFrame>() == 176);
const _: () = assert!(core::mem::align_of::<TrapFrame>() == 8);
const _: () = assert!(core::mem::offset_of!(TrapFrame, vector) == 120);
const _: () = assert!(core::mem::offset_of!(TrapFrame, error_code) == 128);
const _: () = assert!(core::mem::offset_of!(TrapFrame, rip) == 136);
const _: () = assert!(core::mem::offset_of!(TrapFrame, ss) == 168);
