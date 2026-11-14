// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Signal entry and return ABI (PG-7).
//!
//! `SIG_SETUP` registers a per-thread user entry point and a fixed context area relative to the
//! thread's TLS base; `THREAD_KICK` bits are delivered by saving the interrupted machine context
//! into that area and resuming at the entry point. `SIGRETURN` restores the saved context.
//!
//! The context layout mirrors the C36 `mcontext_t` of each architecture. The public C home stays
//! `raven-libc-abi`; this module is the kernel-side mirror so the kernel does not depend on the
//! libc crate. Sizes and offsets are asserted against the C36 values below.

/// `sig_setup(entry: usize, ctx_offset: usize, flags: u32) -> 0`.
///
/// A raw constant, not a [`crate::syscall::Syscall`] variant, so the existing exhaustive matches
/// in the libc crates stay intact. `SIG_SETUP` is self-only and per thread.
pub const SIG_SETUP: usize = 0x80;

/// `sigreturn() -> !` on success; `EINVAL` when the caller is not in a signal.
///
/// Restores the context saved by the last kick delivery. Raw constant for the same reason as
/// [`SIG_SETUP`].
pub const SIGRETURN: usize = 0x82;

/// `SIG_SETUP` flag: enter with the interrupted `sp` instead of a fresh `sp - 128` frame.
pub const ENTRY_SP: u32 = 1;

/// Largest allowed `ctx_offset + size_of::<MContext>()`.
pub const CONTEXT_AREA_LIMIT: usize = 64 * 1024;

/// `MContext` byte size on the compiling architecture (C36 `mcontext_t`).
#[cfg(target_arch = "x86_64")]
pub const MCONTEXT_SIZE: usize = 256;
/// `MContext` byte size on the compiling architecture (C36 `mcontext_t`).
#[cfg(target_arch = "aarch64")]
pub const MCONTEXT_SIZE: usize = 4384;
/// `MContext` byte size on the compiling architecture (C36 `mcontext_t`).
#[cfg(target_arch = "riscv64")]
pub const MCONTEXT_SIZE: usize = 784;

/// x86-64 `REG_R8` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_R8: usize = 0;
/// x86-64 `REG_R9` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_R9: usize = 1;
/// x86-64 `REG_R10` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_R10: usize = 2;
/// x86-64 `REG_R11` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_R11: usize = 3;
/// x86-64 `REG_R12` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_R12: usize = 4;
/// x86-64 `REG_R13` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_R13: usize = 5;
/// x86-64 `REG_R14` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_R14: usize = 6;
/// x86-64 `REG_R15` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_R15: usize = 7;
/// x86-64 `REG_RDI` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_RDI: usize = 8;
/// x86-64 `REG_RSI` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_RSI: usize = 9;
/// x86-64 `REG_RBP` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_RBP: usize = 10;
/// x86-64 `REG_RBX` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_RBX: usize = 11;
/// x86-64 `REG_RDX` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_RDX: usize = 12;
/// x86-64 `REG_RAX` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_RAX: usize = 13;
/// x86-64 `REG_RCX` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_RCX: usize = 14;
/// x86-64 `REG_RSP` (interrupted stack pointer) index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_RSP: usize = 15;
/// x86-64 `REG_RIP` (interrupted program counter) index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_RIP: usize = 16;
/// x86-64 `REG_EFL` (interrupted RFLAGS) index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_EFL: usize = 17;
/// x86-64 `REG_CSGSFS` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_CSGSFS: usize = 18;
/// x86-64 `REG_ERR` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_ERR: usize = 19;
/// x86-64 `REG_TRAPNO` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_TRAPNO: usize = 20;
/// x86-64 `REG_OLDMASK` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_OLDMASK: usize = 21;
/// x86-64 `REG_CR2` index in `gregs`.
#[cfg(target_arch = "x86_64")]
pub const REG_CR2: usize = 22;

/// riscv64 `REG_PC` index in `gregs`.
#[cfg(target_arch = "riscv64")]
pub const REG_PC: usize = 0;
/// riscv64 `REG_RA` index in `gregs`.
#[cfg(target_arch = "riscv64")]
pub const REG_RA: usize = 1;
/// riscv64 `REG_SP` index in `gregs`.
#[cfg(target_arch = "riscv64")]
pub const REG_SP: usize = 2;

/// Kernel-side x86-64 `mcontext_t` (C36: 256 bytes, align 8).
#[cfg(target_arch = "x86_64")]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MContext {
    /// `gregset_t`: 23 general-purpose register slots.
    pub gregs: [u64; 23],
    /// `fpregset_t` pointer; zero until PG-12 saves FP/SIMD state in the frame.
    pub fpregs: u64,
    /// `__reserved1[8]`.
    pub reserved: [u64; 8],
}

/// Kernel-side aarch64 `mcontext_t` (C36: 4384 bytes, align 16).
#[cfg(target_arch = "aarch64")]
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MContext {
    /// Fault address (zero for kick delivery; S45 uses it for faults).
    pub fault_address: u64,
    /// `regs[31]`; index 30 is the link register.
    pub regs: [u64; 31],
    /// Interrupted stack pointer.
    pub sp: u64,
    /// Interrupted program counter.
    pub pc: u64,
    /// Interrupted PSTATE.
    pub pstate: u64,
    /// `long double __reserved[256]` (16 bytes each on aarch64).
    pub reserved: [u128; 256],
}

/// Kernel-side riscv64 `mcontext_t` (C36: 784 bytes, align 16).
#[cfg(target_arch = "riscv64")]
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MContext {
    /// `__gregs[32]`.
    pub gregs: [u64; 32],
    /// `__fpregs`: the 528-byte quad-precision union image.
    pub fpregs: [u8; 528],
}

impl MContext {
    /// All-zero context.
    pub const fn zero() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Self {
                gregs: [0; 23],
                fpregs: 0,
                reserved: [0; 8],
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            Self {
                fault_address: 0,
                regs: [0; 31],
                sp: 0,
                pc: 0,
                pstate: 0,
                reserved: [0; 256],
            }
        }
        #[cfg(target_arch = "riscv64")]
        {
            Self {
                gregs: [0; 32],
                fpregs: [0; 528],
            }
        }
    }

    /// Interrupted program counter.
    #[cfg(target_arch = "x86_64")]
    pub const fn pc(&self) -> u64 {
        self.gregs[REG_RIP]
    }

    /// Interrupted program counter.
    #[cfg(target_arch = "aarch64")]
    pub const fn pc(&self) -> u64 {
        self.pc
    }

    /// Interrupted program counter.
    #[cfg(target_arch = "riscv64")]
    pub const fn pc(&self) -> u64 {
        self.gregs[REG_PC]
    }

    /// Interrupted stack pointer.
    #[cfg(target_arch = "x86_64")]
    pub const fn sp(&self) -> u64 {
        self.gregs[REG_RSP]
    }

    /// Interrupted stack pointer.
    #[cfg(target_arch = "aarch64")]
    pub const fn sp(&self) -> u64 {
        self.sp
    }

    /// Interrupted stack pointer.
    #[cfg(target_arch = "riscv64")]
    pub const fn sp(&self) -> u64 {
        self.gregs[REG_SP]
    }

    /// Set the interrupted program counter.
    #[cfg(target_arch = "x86_64")]
    pub fn set_pc(&mut self, value: u64) {
        self.gregs[REG_RIP] = value;
    }

    /// Set the interrupted program counter.
    #[cfg(target_arch = "aarch64")]
    pub fn set_pc(&mut self, value: u64) {
        self.pc = value;
    }

    /// Set the interrupted program counter.
    #[cfg(target_arch = "riscv64")]
    pub fn set_pc(&mut self, value: u64) {
        self.gregs[REG_PC] = value;
    }

    /// Set the interrupted stack pointer.
    #[cfg(target_arch = "x86_64")]
    pub fn set_sp(&mut self, value: u64) {
        self.gregs[REG_RSP] = value;
    }

    /// Set the interrupted stack pointer.
    #[cfg(target_arch = "aarch64")]
    pub fn set_sp(&mut self, value: u64) {
        self.sp = value;
    }

    /// Set the interrupted stack pointer.
    #[cfg(target_arch = "riscv64")]
    pub fn set_sp(&mut self, value: u64) {
        self.gregs[REG_SP] = value;
    }

    /// One general-purpose register by architectural index.
    #[cfg(any(target_arch = "x86_64", target_arch = "riscv64"))]
    pub const fn reg(&self, index: usize) -> Option<u64> {
        if index < self.gregs.len() {
            Some(self.gregs[index])
        } else {
            None
        }
    }

    /// One general-purpose register by architectural index.
    #[cfg(target_arch = "aarch64")]
    pub const fn reg(&self, index: usize) -> Option<u64> {
        if index < self.regs.len() {
            Some(self.regs[index])
        } else {
            None
        }
    }
}

#[cfg(target_arch = "x86_64")]
const _: () = {
    assert!(core::mem::size_of::<MContext>() == 256);
    assert!(core::mem::align_of::<MContext>() == 8);
    assert!(core::mem::offset_of!(MContext, gregs) == 0);
    assert!(core::mem::offset_of!(MContext, gregs) + REG_RSP * 8 == 120);
    assert!(core::mem::offset_of!(MContext, gregs) + REG_RIP * 8 == 128);
    assert!(core::mem::offset_of!(MContext, fpregs) == 184);
    assert!(core::mem::offset_of!(MContext, reserved) == 192);
};

#[cfg(target_arch = "aarch64")]
const _: () = {
    assert!(core::mem::size_of::<MContext>() == 4384);
    assert!(core::mem::align_of::<MContext>() == 16);
    assert!(core::mem::offset_of!(MContext, fault_address) == 0);
    assert!(core::mem::offset_of!(MContext, regs) == 8);
    assert!(core::mem::offset_of!(MContext, sp) == 256);
    assert!(core::mem::offset_of!(MContext, pc) == 264);
    assert!(core::mem::offset_of!(MContext, pstate) == 272);
    assert!(core::mem::offset_of!(MContext, reserved) == 288);
};

#[cfg(target_arch = "riscv64")]
const _: () = {
    assert!(core::mem::size_of::<MContext>() == 784);
    assert!(core::mem::align_of::<MContext>() == 16);
    assert!(core::mem::offset_of!(MContext, gregs) == 0);
    assert!(core::mem::offset_of!(MContext, gregs) + REG_PC * 8 == 0);
    assert!(core::mem::offset_of!(MContext, gregs) + REG_SP * 8 == 16);
    assert!(core::mem::offset_of!(MContext, fpregs) == 256);
};

#[cfg(test)]
#[path = "signal_ut.rs"]
mod tests;
