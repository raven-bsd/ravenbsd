// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 system-call entry support.
//!
//! Unlike x86-64's `syscall` MSRs, `svc #0` needs no per-CPU configuration: EL0 may always
//! execute it while `HCR_EL2.TGE` is clear (the loader runs without a hypervisor). The exception
//! arrives in the lower-EL AArch64 synchronous vector with `ESR_EL1.EC = 0x15` and is dispatched
//! by [`dispatch`].
//!
//! The exception entry masks DAIF. The x86-64 syscall entry re-enables IF for the memory, scheme
//! IPC and futex syscalls so a target CPU spinning on the address-space lock can still take an SMP
//! TLB IPI (`kernel/src/sched/context.rs` `sched_syscall_enter`). aarch64 needs the same
//! interruptible set: the TLB shootdown is an SGI (IRQ), and a peer in `UNMAP` with DAIF masked
//! would otherwise time out waiting behind that lock. Every other syscall keeps the entry mask.

use core::arch::asm;

use raven_abi::syscall::Syscall;

use crate::arch::aarch64::vectors::TrapFrame;

/// Read the current DAIF mask.
fn read_daif() -> u64 {
    let flags: u64;
    // SAFETY: DAIF is an EL1 system register; the value is restored by `write_daif`.
    unsafe {
        asm!("mrs {}, daif", out(reg) flags, options(nomem, nostack, preserves_flags));
    }
    flags
}

/// Restore a DAIF mask captured before `enable_irq`.
fn write_daif(flags: u64) {
    // SAFETY: DAIF is an EL1 system register; `flags` came from `read_daif` on this CPU.
    unsafe {
        asm!("msr daif, {}", in(reg) flags, options(nomem, nostack, preserves_flags));
    }
}

/// Unmask IRQ (DAIF.I) so a pending TLB SGI can be taken while the syscall waits.
fn enable_irq() {
    // SAFETY: the vectors and GIC are installed before user syscalls run; the caller restores the
    // saved DAIF before returning to the vector path.
    unsafe {
        asm!("msr daifclr, #2", options(nomem, nostack, preserves_flags));
    }
}

/// The same interruptible set as the x86-64 syscall entry.
///
/// These syscalls can be long or wait on a lock; keeping them non-preemptible would block a remote
/// TLB acknowledgement. The generic memory/IPC paths use IRQ-safe locks, matching x86.
fn interruptible(number: usize) -> bool {
    matches!(
        Syscall::from_usize(number),
        Some(
            Syscall::MAP
                | Syscall::UNMAP
                | Syscall::PROTECT
                | Syscall::THREAD_CREATE
                | Syscall::THREAD_EXIT
                | Syscall::EXIT
                | Syscall::FUTEX
                | Syscall::SCHEME_REGISTER
                | Syscall::OPEN
                | Syscall::READ
                | Syscall::WRITE
                | Syscall::CLOSE
                | Syscall::DUP
                | Syscall::FSTAT
                | Syscall::CALL
                | Syscall::RECV
                | Syscall::SEND
        )
    ) || matches!(
        number,
        crate::syscall::ipc::PREAD | crate::syscall::ipc::PWRITE | crate::syscall::ipc::SEEK
    )
}

/// Run `f` with IRQ unmasked, restoring the entry DAIF afterwards.
///
/// The aarch64 user page-fault path takes the address-space lock like `unmap`; a target spinning
/// on that lock with DAIF masked cannot take the TLB SGI. This is the aarch64 equivalent of the
/// x86 `PageFaultGuard` (`kernel/src/process/mod.rs`).
pub(crate) fn with_irq_enabled<T>(f: impl FnOnce() -> T) -> T {
    let flags = read_daif();
    enable_irq();
    let result = f();
    write_daif(flags);
    result
}

/// Dispatch an SVC, enabling IRQ for the x86-equivalent interruptible syscall set.
pub fn dispatch(frame: &mut TrapFrame) {
    let number = frame
        .x
        .get(8)
        .copied()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(usize::MAX);
    if !interruptible(number) {
        crate::syscall::svc_dispatch(frame);
        return;
    }
    let flags = read_daif();
    enable_irq();
    crate::syscall::svc_dispatch(frame);
    write_daif(flags);
}

/// No architecture setup is required for `svc`.
pub fn init() {}

#[cfg(test)]
#[path = "syscall_ut.rs"]
mod tests;
