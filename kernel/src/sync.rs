// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interrupt-safe locks and scheduler critical sections.
//!
//! Every lock here disables interrupts and increments the CPU's preemption counter, so the
//! scheduler cannot switch away while kernel state is being mutated. `IrqSpinLock` is a real
//! spinlock (needed for the S10 AP bring-up) whose guard restores the saved `RFLAGS.IF` on drop.
//! `critical()` is the same critical section without a spin flag, used by the scheduler itself
//! because the run queue is per-CPU and is only mutated with interrupts off.
//!
//! The preemption counter follows the running thread: `sched::context::switch` stores the
//! outgoing thread's counter and installs the incoming thread's value. A guard created before a
//! blocking context switch is therefore still live, and is dropped, when that thread resumes.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

use crate::sched::smp::percpu;

const RFLAGS_IF: u64 = 1 << 9;

/// Disable interrupts and return the previous flags.
#[inline]
fn disable_interrupts() -> u64 {
    let flags: u64;
    // SAFETY: cli is ring-0 only and we run in the kernel.
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {}",
            "cli",
            out(reg) flags,
            options(nomem, nostack),
        );
    }
    flags
}

/// Restore the `IF` bit from `flags`.
#[inline]
fn restore_interrupts(flags: u64) {
    // SAFETY: sti/cli are ring-0 only; the caller passes flags captured by `disable_interrupts`.
    unsafe {
        if flags & RFLAGS_IF != 0 {
            core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
        } else {
            core::arch::asm!("cli", options(nomem, nostack));
        }
    }
}

/// Current preemption nesting depth of the running thread on this CPU.
#[inline]
pub fn preempt_count() -> u32 {
    percpu::preempt_count()
}

/// Install a thread's saved preemption depth during a context switch.
///
/// # Safety
///
/// Must only be called from the context-switch path with interrupts disabled and with exclusive
/// access to the run queue.
#[inline]
pub(crate) unsafe fn set_preempt_count(value: u32) {
    percpu::set_preempt_count(value);
}

/// True when no lock or scheduler critical section is held.
#[inline]
pub fn preemption_enabled() -> bool {
    preempt_count() == 0
}

/// Enable interrupts.
///
/// # Safety
///
/// Only call once the interrupt descriptor table and the device gates are installed.
#[inline]
pub unsafe fn enable_interrupts() {
    // SAFETY: the caller guarantees the IDT and gates are ready.
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

/// A scheduler critical section guard.
///
/// Interrupts are off and `preempt_count` is incremented until the guard drops. The guard may be
/// live across a context switch: the incoming thread observes its own saved counter and the
/// outgoing thread's guard is restored when it is scheduled again. This is the documented
/// run-queue handoff.
pub struct CriticalGuard {
    flags: u64,
}

impl CriticalGuard {
    /// Enter the critical section.
    pub fn new() -> Self {
        let flags = disable_interrupts();
        percpu::preempt_inc();
        Self { flags }
    }
}

impl Default for CriticalGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CriticalGuard {
    fn drop(&mut self) {
        percpu::preempt_dec();
        restore_interrupts(self.flags);
    }
}

/// Enter a scheduler critical section: interrupts off, `preempt_count` incremented.
#[inline]
pub fn critical() -> CriticalGuard {
    CriticalGuard::new()
}

/// A spinlock that disables interrupts while it is held.
pub struct IrqSpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

// SAFETY: the lock serialises every access to `data`; `T: Send` makes moving it across contexts
// safe. The single CPU's interrupt disable makes the guard non-reentrant.
unsafe impl<T: Send> Sync for IrqSpinLock<T> {}

impl<T> IrqSpinLock<T> {
    /// Create an unlocked lock.
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(value),
        }
    }

    /// Acquire the lock, disabling interrupts until the guard drops.
    pub fn lock(&self) -> IrqGuard<'_, T> {
        let flags = disable_interrupts();
        while self.locked.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        percpu::preempt_inc();
        IrqGuard { lock: self, flags }
    }
}

/// Guard returned by [`IrqSpinLock::lock`].
pub struct IrqGuard<'a, T> {
    lock: &'a IrqSpinLock<T>,
    flags: u64,
}

impl<T> Deref for IrqGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard proves exclusive access for its lifetime.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for IrqGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard proves exclusive access for its lifetime.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for IrqGuard<'_, T> {
    fn drop(&mut self) {
        percpu::preempt_dec();
        self.lock.locked.store(false, Ordering::Release);
        restore_interrupts(self.flags);
    }
}
