// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure delivery decisions: pending selection, fault override and the default action.
//!
//! CLEANROOM from ticket sections 4.5 and 4.6. Standard signals sort before real-time signals
//! because their numbers are lower; within one number the process queue is FIFO, which is the
//! kernel/procd side. The entry path uses these functions and never inspects a disposition here.

use crate::defaults::{DefaultAction, default_action};
use crate::frame::{fault_to_signal, si_signo};
use crate::kernel::{FaultKind, FaultRecord};
use crate::sigset::APP_MASK;
use raven_libc_abi::structs::SiginfoT;

/// The bits of `pending` deliverable under `mask` (internal bits never deliver).
#[must_use]
pub fn deliverable(pending: u64, mask: u64) -> u64 {
    pending & !mask & APP_MASK
}

/// The lowest set signal number, or 0.
#[must_use]
pub fn lowest(bits: u64) -> i32 {
    if bits == 0 {
        0
    } else {
        (bits.trailing_zeros() as i32).wrapping_add(1)
    }
}

/// Whether the signal is one the application may see.
#[must_use]
pub fn application_signal(sig: i32) -> bool {
    (1..=64).contains(&sig) && !(32..=34).contains(&sig)
}

/// What to do with a queued signal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    /// No deliverable signal.
    Nothing,
    /// Discard (`SIG_IGN`).
    Ignore,
    /// Apply the default action.
    Default(DefaultAction),
    /// Run the handler at this address.
    Handler(usize),
    /// Defer until the critical section ends.
    Defer,
}

/// Decide one signal.
#[must_use]
pub fn decide(handler: usize, sig: i32, critical: bool, fault: bool) -> Decision {
    if sig == 0 {
        return Decision::Nothing;
    }
    if critical {
        return Decision::Defer;
    }
    if handler == 1 {
        return Decision::Ignore;
    }
    if handler == 0 {
        if fault {
            // A synchronous fault with a default disposition always terminates; the table gives
            // core or terminate for the wait status (4.11).
            return Decision::Default(default_action(sig).unwrap_or(DefaultAction::Core));
        }
        return default_action(sig).map_or(Decision::Nothing, Decision::Default);
    }
    Decision::Handler(handler)
}

/// The fault signal record for a raw kernel fault.
#[must_use]
pub fn fault_signal(fault: &FaultRecord) -> Option<(i32, SiginfoT)> {
    let kind = FaultKind::from_u32(fault.kind)?;
    let (sig, _code) = fault_to_signal(kind);
    crate::frame::fault_siginfo(fault).map(|info| (sig, info))
}

/// A fault signal ignores the mask (4.5 step 5).
#[must_use]
pub fn fault_ignores_mask(_sig: i32) -> bool {
    true
}

/// The `si_signo` of a record (used by the pending-info check).
#[must_use]
pub fn info_signal(info: &SiginfoT) -> i32 {
    si_signo(info)
}

/// A second arrival of a queued non-real-time signal is dropped (4.7 one-slot rule).
#[must_use]
pub fn drop_duplicate(sig: i32, already_pending: u64) -> bool {
    sig < 32 && already_pending & crate::critical::signal_bit(sig) != 0
}

#[cfg(test)]
#[path = "deliver_ut.rs"]
mod deliver_ut;
