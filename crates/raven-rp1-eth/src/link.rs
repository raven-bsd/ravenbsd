// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The Ethernet link/autonegotiation state machine and the interrupt cause model.
//!
//! The machine sequences PHY polling, MAC reconfiguration and the link notification. The RP1
//! interrupt register bits are not public in this workspace; [`InterruptState`] models causes,
//! masks and acknowledgement, and the board bring-up maps the real bits onto it.

use core::fmt;

/// A link speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    /// 10 Mbit/s.
    M10,
    /// 100 Mbit/s.
    M100,
    /// 1000 Mbit/s.
    M1000,
}

/// A duplex mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Duplex {
    /// Half duplex.
    Half,
    /// Full duplex.
    Full,
}

/// The link state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// No link.
    Down,
    /// Autonegotiation is in progress.
    Negotiating,
    /// Link is up.
    Up {
        /// Negotiated speed.
        speed: Speed,
        /// Negotiated duplex.
        duplex: Duplex,
    },
    /// A fault stopped the machine until `Start`.
    Faulted,
}

/// What the machine observes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkEvent {
    /// Begin (or restart) autonegotiation.
    Start,
    /// The PHY reported autonegotiation complete with these parameters.
    AutonegComplete {
        /// Negotiated speed.
        speed: Speed,
        /// Negotiated duplex.
        duplex: Duplex,
    },
    /// The PHY lost the link.
    LinkDown,
    /// A poll did not answer in time.
    Timeout,
    /// The PHY reported a fault that polling cannot fix.
    PhyFault,
}

/// What the caller does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAction {
    /// Poll the PHY for its link status.
    PollPhy,
    /// The link came up; configure the MAC.
    LinkUp {
        /// Negotiated speed.
        speed: Speed,
        /// Negotiated duplex.
        duplex: Duplex,
    },
    /// The link went down; notify the stack.
    LinkDown,
    /// The machine stopped.
    Fault,
}

/// Why a link transition is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// The event does not belong to the current state.
    UnexpectedEvent {
        /// Current state.
        state: LinkState,
        /// Rejected event.
        event: LinkEvent,
    },
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEvent { state, event } => {
                write!(f, "event {event:?} is not valid while {state:?}")
            }
        }
    }
}

impl core::error::Error for LinkError {}

/// The link machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkMachine {
    state: LinkState,
    timeouts: u16,
    max_timeouts: u16,
}

impl LinkMachine {
    /// A down machine that tolerates `max_timeouts` unanswered polls per attempt.
    #[must_use]
    pub const fn new(max_timeouts: u16) -> Self {
        Self {
            state: LinkState::Down,
            timeouts: 0,
            max_timeouts,
        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> LinkState {
        self.state
    }

    /// Timeouts since the last progress.
    #[must_use]
    pub const fn timeouts(&self) -> u16 {
        self.timeouts
    }

    /// Feed one event.
    ///
    /// # Errors
    ///
    /// Returns [`LinkError::UnexpectedEvent`] when the event does not belong to the current
    /// state; the state is unchanged.
    pub fn on(&mut self, event: LinkEvent) -> Result<LinkAction, LinkError> {
        match event {
            LinkEvent::Start => {
                if matches!(self.state, LinkState::Negotiating | LinkState::Up { .. }) {
                    return Err(self.unexpected(event));
                }
                self.state = LinkState::Negotiating;
                self.timeouts = 0;
                Ok(LinkAction::PollPhy)
            }
            LinkEvent::AutonegComplete { speed, duplex } => {
                if self.state != LinkState::Negotiating {
                    return Err(self.unexpected(event));
                }
                self.state = LinkState::Up { speed, duplex };
                self.timeouts = 0;
                Ok(LinkAction::LinkUp { speed, duplex })
            }
            LinkEvent::LinkDown => match self.state {
                LinkState::Negotiating | LinkState::Up { .. } => {
                    self.state = LinkState::Down;
                    self.timeouts = 0;
                    Ok(LinkAction::LinkDown)
                }
                LinkState::Down | LinkState::Faulted => Err(self.unexpected(event)),
            },
            LinkEvent::Timeout => {
                if self.state != LinkState::Negotiating {
                    return Err(self.unexpected(event));
                }
                self.timeouts = self.timeouts.saturating_add(1);
                if self.timeouts > self.max_timeouts {
                    self.state = LinkState::Faulted;
                    return Ok(LinkAction::Fault);
                }
                Ok(LinkAction::PollPhy)
            }
            LinkEvent::PhyFault => match self.state {
                LinkState::Negotiating | LinkState::Up { .. } | LinkState::Down => {
                    self.state = LinkState::Faulted;
                    Ok(LinkAction::Fault)
                }
                LinkState::Faulted => Err(self.unexpected(event)),
            },
        }
    }

    fn unexpected(&self, event: LinkEvent) -> LinkError {
        LinkError::UnexpectedEvent {
            state: self.state,
            event,
        }
    }
}

/// One interrupt cause set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Interrupts {
    /// A receive descriptor completed.
    pub rx_complete: bool,
    /// A transmit descriptor completed.
    pub tx_complete: bool,
    /// The link changed.
    pub link_change: bool,
    /// The device reported an error.
    pub error: bool,
}

impl Interrupts {
    /// No causes.
    pub const NONE: Self = Self {
        rx_complete: false,
        tx_complete: false,
        link_change: false,
        error: false,
    };

    /// Every cause.
    pub const ALL: Self = Self {
        rx_complete: true,
        tx_complete: true,
        link_change: true,
        error: true,
    };

    /// True when any cause is set.
    #[must_use]
    pub const fn any(self) -> bool {
        self.rx_complete || self.tx_complete || self.link_change || self.error
    }

    const fn bits(self) -> u8 {
        (self.rx_complete as u8)
            | ((self.tx_complete as u8) << 1)
            | ((self.link_change as u8) << 2)
            | ((self.error as u8) << 3)
    }

    const fn from_bits(bits: u8) -> Self {
        Self {
            rx_complete: bits & 0b0001 != 0,
            tx_complete: bits & 0b0010 != 0,
            link_change: bits & 0b0100 != 0,
            error: bits & 0b1000 != 0,
        }
    }
}

/// The pending/mask interrupt state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InterruptState {
    pending: Interrupts,
    mask: Interrupts,
}

impl InterruptState {
    /// Nothing pending, nothing masked.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: Interrupts::NONE,
            mask: Interrupts::NONE,
        }
    }

    /// Raise causes.
    pub fn raise(&mut self, causes: Interrupts) {
        self.pending = Interrupts::from_bits(self.pending.bits() | causes.bits());
    }

    /// Replace the mask; a set bit suppresses that cause.
    pub fn set_mask(&mut self, mask: Interrupts) {
        self.mask = mask;
    }

    /// The mask.
    #[must_use]
    pub const fn mask(&self) -> Interrupts {
        self.mask
    }

    /// The raw pending causes.
    #[must_use]
    pub const fn pending(&self) -> Interrupts {
        self.pending
    }

    /// The causes that reach the CPU: pending and not masked.
    #[must_use]
    pub const fn active(&self) -> Interrupts {
        Interrupts::from_bits(self.pending.bits() & !self.mask.bits())
    }

    /// True when an unmasked cause is pending.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active().any()
    }

    /// Acknowledge (clear) causes.
    pub fn acknowledge(&mut self, causes: Interrupts) {
        self.pending = Interrupts::from_bits(self.pending.bits() & !causes.bits());
    }
}

#[cfg(test)]
#[path = "link_ut.rs"]
mod tests;
