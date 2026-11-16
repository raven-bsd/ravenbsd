// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The xHCI port and roothub state machine.
//!
//! The state names and transitions follow the xHCI 1.1 port model: a disconnected port enters
//! polling, a completed reset enables it, a suspend/resume pair moves it through the link states,
//! and connect/enable/reset/link/over-current changes raise the corresponding change bits. The
//! VL805/BCM2711 integration is UNVERIFIED; only the model transitions are tested.

use core::fmt;

/// Largest number of ports the model holds.
pub const MAX_PORTS: usize = 8;

/// The port state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortState {
    /// Nothing is attached.
    NotConnected,
    /// Attached but not enabled.
    Disabled,
    /// The receiver detection phase of a reset.
    RxDetect,
    /// The polling phase after a connect.
    Polling,
    /// Enabled and operating.
    Enabled,
    /// Suspended.
    Suspended,
    /// A resume is being signalled.
    Resumed,
    /// A port reset is in progress.
    Resetting,
    /// The port is disabled by an error or over-current.
    Inactive,
}

/// What the port observes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortEvent {
    /// A device attached.
    Connect,
    /// The device detached.
    Disconnect,
    /// Start a port reset.
    Reset,
    /// A port reset completed.
    ResetComplete,
    /// Suspend the port.
    Suspend,
    /// Signal a resume.
    Resume,
    /// The link reached the enabled state.
    LinkUp,
    /// Over-current detected.
    Overcurrent,
    /// A fatal port error.
    Error,
}

/// What the caller does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortAction {
    /// Start link training/reset.
    Reset,
    /// The port is enabled.
    Enabled,
    /// The device detached.
    Disconnected,
    /// The port is suspended.
    Suspended,
    /// The resume signal is active.
    Resumed,
    /// The port is inactive.
    Fault,
    /// No side effect.
    None,
}

/// The change bits raised since the last acknowledge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortChanges {
    /// Connect status changed.
    pub connect: bool,
    /// Enabled status changed.
    pub enable: bool,
    /// Reset completed.
    pub reset: bool,
    /// Link state changed.
    pub link: bool,
    /// Over-current changed.
    pub overcurrent: bool,
}

impl PortChanges {
    /// No changes.
    pub const NONE: Self = Self {
        connect: false,
        enable: false,
        reset: false,
        link: false,
        overcurrent: false,
    };

    /// True when any change bit is set.
    #[must_use]
    pub const fn any(self) -> bool {
        self.connect || self.enable || self.reset || self.link || self.overcurrent
    }
}

/// Why a port transition is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortError {
    /// The number of ports is zero or above [`MAX_PORTS`].
    PortCount(u8),
    /// The port index is outside the roothub.
    PortOutOfRange(u8),
    /// The event does not belong to the current state.
    UnexpectedEvent {
        /// Current state.
        state: PortState,
        /// Rejected event.
        event: PortEvent,
    },
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PortCount(ports) => write!(f, "port count {ports} is outside 1..={MAX_PORTS}"),
            Self::PortOutOfRange(port) => write!(f, "port {port} is outside the roothub"),
            Self::UnexpectedEvent { state, event } => {
                write!(f, "event {event:?} is not valid while {state:?}")
            }
        }
    }
}

impl core::error::Error for PortError {}

/// One port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortMachine {
    state: PortState,
    changes: PortChanges,
}

impl Default for PortMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl PortMachine {
    /// A disconnected port.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: PortState::NotConnected,
            changes: PortChanges::NONE,
        }
    }

    /// The current state.
    #[must_use]
    pub const fn state(&self) -> PortState {
        self.state
    }

    /// The change bits.
    #[must_use]
    pub const fn changes(&self) -> PortChanges {
        self.changes
    }

    /// Clear the change bits (the write-1-to-clear ack).
    pub fn acknowledge(&mut self) {
        self.changes = PortChanges::NONE;
    }

    /// Feed one event.
    ///
    /// # Errors
    ///
    /// Returns [`PortError::UnexpectedEvent`] when the event does not belong to the state; the
    /// state is unchanged.
    pub fn on(&mut self, event: PortEvent) -> Result<PortAction, PortError> {
        match event {
            PortEvent::Connect => {
                if self.state != PortState::NotConnected {
                    return Err(self.unexpected(event));
                }
                self.state = PortState::Polling;
                self.changes.connect = true;
                Ok(PortAction::Reset)
            }
            PortEvent::Disconnect => {
                if self.state == PortState::NotConnected {
                    return Err(self.unexpected(event));
                }
                self.state = PortState::NotConnected;
                self.changes.connect = true;
                self.changes.link = true;
                Ok(PortAction::Disconnected)
            }
            PortEvent::Reset => {
                if !matches!(self.state, PortState::Enabled | PortState::Polling) {
                    return Err(self.unexpected(event));
                }
                self.state = PortState::Resetting;
                Ok(PortAction::Reset)
            }
            PortEvent::ResetComplete => {
                if !matches!(self.state, PortState::Resetting | PortState::RxDetect) {
                    return Err(self.unexpected(event));
                }
                self.state = PortState::Enabled;
                self.changes.reset = true;
                self.changes.enable = true;
                self.changes.link = true;
                Ok(PortAction::Enabled)
            }
            PortEvent::LinkUp => {
                if !matches!(self.state, PortState::Polling | PortState::Resumed) {
                    return Err(self.unexpected(event));
                }
                self.state = PortState::Enabled;
                self.changes.enable = true;
                self.changes.link = true;
                Ok(PortAction::Enabled)
            }
            PortEvent::Suspend => {
                if self.state != PortState::Enabled {
                    return Err(self.unexpected(event));
                }
                self.state = PortState::Suspended;
                self.changes.link = true;
                Ok(PortAction::Suspended)
            }
            PortEvent::Resume => {
                if self.state != PortState::Suspended {
                    return Err(self.unexpected(event));
                }
                self.state = PortState::Resumed;
                self.changes.link = true;
                Ok(PortAction::Resumed)
            }
            PortEvent::Overcurrent => {
                self.state = PortState::Inactive;
                self.changes.overcurrent = true;
                Ok(PortAction::Fault)
            }
            PortEvent::Error => {
                if self.state == PortState::NotConnected {
                    return Err(self.unexpected(event));
                }
                self.state = PortState::Inactive;
                Ok(PortAction::Fault)
            }
        }
    }

    fn unexpected(&self, event: PortEvent) -> PortError {
        PortError::UnexpectedEvent {
            state: self.state,
            event,
        }
    }
}

/// The root hub holding the controller ports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roothub {
    ports: [PortMachine; MAX_PORTS],
    count: u8,
}

impl Roothub {
    /// Build a roothub with `ports` ports.
    ///
    /// # Errors
    ///
    /// Returns [`PortError::PortCount`] when `ports` is zero or above [`MAX_PORTS`].
    pub fn new(count: u8) -> Result<Self, PortError> {
        if count == 0 || usize::from(count) > MAX_PORTS {
            return Err(PortError::PortCount(count));
        }
        Ok(Self {
            ports: [PortMachine::new(); MAX_PORTS],
            count,
        })
    }

    /// The number of ports.
    #[must_use]
    pub const fn count(&self) -> u8 {
        self.count
    }

    /// One 1-based port.
    ///
    /// # Errors
    ///
    /// Returns [`PortError::PortOutOfRange`] for 0 or a port above the hub size.
    pub fn port(&self, port: u8) -> Result<&PortMachine, PortError> {
        if port == 0 || port > self.count {
            return Err(PortError::PortOutOfRange(port));
        }
        self.ports
            .get(usize::from(port) - 1)
            .ok_or(PortError::PortOutOfRange(port))
    }

    /// One mutable 1-based port.
    ///
    /// # Errors
    ///
    /// Returns [`PortError::PortOutOfRange`] for 0 or a port above the hub size.
    pub fn port_mut(&mut self, port: u8) -> Result<&mut PortMachine, PortError> {
        if port == 0 || port > self.count {
            return Err(PortError::PortOutOfRange(port));
        }
        self.ports
            .get_mut(usize::from(port) - 1)
            .ok_or(PortError::PortOutOfRange(port))
    }

    /// Feed an event to one port.
    ///
    /// # Errors
    ///
    /// Returns the port range or transition error.
    pub fn on(&mut self, port: u8, event: PortEvent) -> Result<PortAction, PortError> {
        self.port_mut(port)?.on(event)
    }
}

#[cfg(test)]
#[path = "port_ut.rs"]
mod tests;
