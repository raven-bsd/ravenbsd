// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded error recovery for the GENET frame path.
//!
//! A malformed descriptor or a short burst of DMA errors should end the current batch, not spin
//! the interrupt loop forever. The policy counts consecutive errors and ring resets, then reports
//! a bounded action the caller can implement.

/// What the driver should do after one error observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Continue processing; the threshold is not reached.
    None,
    /// Reset the ring state and refill descriptors.
    ResetRings,
    /// The policy exhausted its reset budget; the link needs a hard reset or a board log.
    Fault,
}

/// A consecutive-error and reset-budget policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorRecovery {
    consecutive: u32,
    resets: u32,
    max_consecutive: u32,
    max_resets: u32,
}

impl ErrorRecovery {
    /// Create a policy. Zero thresholds are clamped to one.
    #[must_use]
    pub const fn new(max_consecutive: u32, max_resets: u32) -> Self {
        Self {
            consecutive: 0,
            resets: 0,
            max_consecutive: if max_consecutive == 0 {
                1
            } else {
                max_consecutive
            },
            max_resets: if max_resets == 0 { 1 } else { max_resets },
        }
    }

    /// Consecutive errors observed since the last success or reset.
    #[must_use]
    pub const fn consecutive(self) -> u32 {
        self.consecutive
    }

    /// Ring resets granted so far.
    #[must_use]
    pub const fn resets(self) -> u32 {
        self.resets
    }

    /// The consecutive-error threshold.
    #[must_use]
    pub const fn max_consecutive(self) -> u32 {
        self.max_consecutive
    }

    /// The reset budget.
    #[must_use]
    pub const fn max_resets(self) -> u32 {
        self.max_resets
    }

    /// Record one error observation.
    pub fn record_error(&mut self) -> RecoveryAction {
        self.consecutive = self.consecutive.saturating_add(1);
        if self.consecutive < self.max_consecutive {
            return RecoveryAction::None;
        }
        self.consecutive = 0;
        if self.resets >= self.max_resets {
            return RecoveryAction::Fault;
        }
        self.resets = self.resets.saturating_add(1);
        RecoveryAction::ResetRings
    }

    /// Record a clean observation and clear the consecutive counter.
    pub fn record_success(&mut self) {
        self.consecutive = 0;
    }
}

impl Default for ErrorRecovery {
    fn default() -> Self {
        Self::new(8, 3)
    }
}

#[cfg(test)]
#[path = "recovery_ut.rs"]
mod tests;
