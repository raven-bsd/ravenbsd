// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side model of the lock-order rule for inter-processor interrupt (IPI) waits.
//!
//! The TLB shootdown waits for every other CPU to acknowledge an IPI. A target CPU that is
//! spinning on a raw spinlock with interrupts masked cannot run the IPI handler, so the shooter
//! waits out its deadline and logs `flushes=0`. The same shape appears on x86-64 (syscall entry
//! clears `RFLAGS.IF`) and aarch64 (SVC entry masks `PSTATE.DAIF`).
//!
//! This module encodes the rule rather than the kernel implementation:
//!
//! * a raw `spin::Mutex` such as the process address-space lock is never held across
//!   [`Event::WaitForIpi`];
//! * an interrupt-safe active-request guard may serialise the shooter, because it re-enables
//!   interrupts while waiting and the IPI handler never takes it;
//! * a target can acknowledge only while its interrupts are enabled.
//!
//! S113's syscall-entry interrupt enable is the delivery mitigation for the target side. It
//! does not discharge the lock-order rule: the address-space lock is still held across the
//! wait, so a target that is interrupt-masked for any other reason still times out. The
//! regression tests keep those two statements separate.

/// A lock class that can be held while a CPU waits for an IPI acknowledgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockKind {
    /// A raw `spin::Mutex` such as the process address-space lock. It masks neither interrupts
    /// nor preemption and must never be held across an IPI wait.
    RawSpin,
    /// An interrupt-safe active-request guard (`ActiveGuard`/`IrqSpinLock`) that re-enables
    /// interrupts while waiting. It may serialise the shooter only because the handler never
    /// takes it.
    InterruptSafe,
}

/// One event in a lock-order trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Acquire a lock of the given kind.
    Acquire(LockKind),
    /// Release a lock of the given kind.
    Release(LockKind),
    /// Wait for a remote CPU to acknowledge an IPI.
    WaitForIpi,
}

/// A lock-order rule violation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Violation {
    /// A raw spinlock was held across an IPI wait.
    RawSpinAcrossIpiWait,
}

/// Check a trace against the no-spinlock-across-an-IPI-wait rule.
///
/// The check returns the first violation, or `Ok(())` when every wait runs with no raw spinlock
/// held. It is deliberately independent of the kernel: the audit maps real call sites into
/// traces and the test suite keeps the rule executable.
///
/// # Errors
///
/// Returns [`Violation::RawSpinAcrossIpiWait`] when [`Event::WaitForIpi`] appears while a
/// [`LockKind::RawSpin`] acquisition is still live.
pub fn check(trace: &[Event]) -> Result<(), Violation> {
    let mut held_raw = 0u32;
    for event in trace {
        match event {
            Event::Acquire(LockKind::RawSpin) => {
                held_raw = held_raw.saturating_add(1);
            }
            Event::Release(LockKind::RawSpin) => {
                held_raw = held_raw.saturating_sub(1);
            }
            Event::WaitForIpi if held_raw > 0 => {
                return Err(Violation::RawSpinAcrossIpiWait);
            }
            Event::Acquire(LockKind::InterruptSafe)
            | Event::Release(LockKind::InterruptSafe)
            | Event::WaitForIpi => {}
        }
    }
    Ok(())
}

/// The target CPU's interrupt state while the shooter waits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetInterrupts {
    /// The IPI handler can run even while the target spins on a lock.
    Enabled,
    /// An exception/syscall entry or a masked critical section has deferred the IPI.
    Masked,
}

/// Model the target side of a TLB IPI acknowledgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    interrupts: TargetInterrupts,
    spinning_on_raw_spin: bool,
}

impl Target {
    /// The S106/S118 pre-fix target: a syscall/SVC entry masked interrupts while the peer spins
    /// on the address-space raw spinlock.
    #[must_use]
    pub const fn masked_spinner() -> Self {
        Self {
            interrupts: TargetInterrupts::Masked,
            spinning_on_raw_spin: true,
        }
    }

    /// The S113 target: syscall entry enabled interrupts, so the spinning peer can take the IPI.
    #[must_use]
    pub const fn interruptible_spinner() -> Self {
        Self {
            interrupts: TargetInterrupts::Enabled,
            spinning_on_raw_spin: true,
        }
    }

    /// True when the target can run the IPI handler without leaving its spin.
    #[must_use]
    pub const fn can_acknowledge(&self) -> bool {
        matches!(self.interrupts, TargetInterrupts::Enabled)
    }

    /// True when the target is waiting for the address-space raw spinlock.
    #[must_use]
    pub const fn is_spinning_on_raw_spin(&self) -> bool {
        self.spinning_on_raw_spin
    }

    /// The S113 mitigation: enable interrupts while the target keeps spinning.
    pub fn enable_interrupts(&mut self) {
        self.interrupts = TargetInterrupts::Enabled;
    }

    /// Wait for an acknowledgement. `None` means the 100 ms deadline expired first.
    #[must_use]
    pub fn wait(&self, deadline_expired: bool) -> Option<()> {
        if self.can_acknowledge() && !deadline_expired {
            Some(())
        } else {
            None
        }
    }
}

/// A target with interrupts enabled but no pending IPI does not acknowledge until one is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitOutcome {
    /// The target acknowledged the published sequence.
    Acknowledged,
    /// The deadline expired with the target still masked.
    TimedOut,
}

/// Runs the target-side acknowledgement model for one IPI.
///
/// This is the small state machine the audit uses for the S118 `flushes=0` signature: a masked
/// target times out, an interruptible target acknowledges, and enabling interrupts after the
/// deadline cannot make the old sequence acknowledge.
#[must_use]
pub fn acknowledge(target: Target, deadline_expired: bool) -> WaitOutcome {
    if target.wait(deadline_expired).is_some() {
        WaitOutcome::Acknowledged
    } else {
        WaitOutcome::TimedOut
    }
}

#[cfg(test)]
#[path = "ipi_order_ut.rs"]
mod tests;
