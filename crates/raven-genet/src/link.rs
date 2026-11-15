// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PHY link-state tracker with sample debouncing.
//!
//! A single `BMSR`/`AUXSTS` read can transiently report the wrong state while autonegotiation
//! settles. The tracker requires a configurable number of consecutive identical samples before
//! it reports an up/down/speed change, so the driver does not flap `UMAC_CMD` or the `netd`
//! link state on one noisy poll.

use crate::phy::{Duplex, LinkError, LinkStatus, Speed};

/// The stable link state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// No sample has been accepted yet.
    Unknown,
    /// The link is down.
    Down,
    /// The link is up at a resolved speed and duplex.
    Up {
        /// Resolved speed.
        speed: Speed,
        /// Resolved duplex.
        duplex: Duplex,
    },
}

/// A link transition after the debounce threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkEvent {
    /// The link came up.
    Up {
        /// Resolved speed.
        speed: Speed,
        /// Resolved duplex.
        duplex: Duplex,
    },
    /// The link went down.
    Down,
    /// The link stayed up but the speed or duplex changed.
    Changed {
        /// Resolved speed.
        speed: Speed,
        /// Resolved duplex.
        duplex: Duplex,
    },
}

/// A debounced link tracker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkTracker {
    stable: LinkState,
    candidate: Option<LinkState>,
    samples: u8,
    debounce: u8,
}

impl LinkTracker {
    /// Create a tracker that requires `debounce` consecutive samples (at least one).
    #[must_use]
    pub const fn new(debounce: u8) -> Self {
        Self {
            stable: LinkState::Unknown,
            candidate: None,
            samples: 0,
            debounce: if debounce == 0 { 1 } else { debounce },
        }
    }

    /// The last accepted stable state.
    #[must_use]
    pub const fn state(self) -> LinkState {
        self.stable
    }

    /// The number of samples required for a transition.
    #[must_use]
    pub const fn debounce(self) -> u8 {
        self.debounce
    }

    /// Feed one `BMSR`/`AUXSTS` sample.
    ///
    /// Returns `Ok(None)` while the candidate is still below the threshold or the sample matches
    /// the stable state. An unresolved link returns [`LinkError`] and leaves the tracker intact.
    pub fn observe(&mut self, bmsr: u16, auxsts: u16) -> Result<Option<LinkEvent>, LinkError> {
        let status = LinkStatus::resolve(bmsr, auxsts)?;
        let target = if status.up {
            let Some(speed) = status.speed else {
                return Err(LinkError::UnknownResolution(0));
            };
            let Some(duplex) = status.duplex else {
                return Err(LinkError::UnknownResolution(0));
            };
            LinkState::Up { speed, duplex }
        } else {
            LinkState::Down
        };

        if target == self.stable {
            self.candidate = None;
            self.samples = 0;
            return Ok(None);
        }

        if self.candidate == Some(target) {
            self.samples = self.samples.saturating_add(1);
        } else {
            self.candidate = Some(target);
            self.samples = 1;
        }
        if self.samples < self.debounce {
            return Ok(None);
        }

        let previous = self.stable;
        self.stable = target;
        self.candidate = None;
        self.samples = 0;
        Ok(Some(match (previous, target) {
            (_, LinkState::Down) => LinkEvent::Down,
            (LinkState::Up { .. }, LinkState::Up { speed, duplex }) => {
                LinkEvent::Changed { speed, duplex }
            }
            (_, LinkState::Up { speed, duplex }) => LinkEvent::Up { speed, duplex },
            (_, LinkState::Unknown) => return Ok(None),
        }))
    }
}

impl Default for LinkTracker {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
#[path = "link_ut.rs"]
mod tests;
