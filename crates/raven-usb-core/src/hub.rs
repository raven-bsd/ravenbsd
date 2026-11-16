// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB hub/root-port state.
//!
//! This is the host-side port state a USB core needs before it can enumerate a device: attach,
//! power, reset, enable, suspend/resume, over-current and the change bits. The speed is recorded
//! at attach time; the device address and configuration are recorded only after the core assigns
//! them. The xHCI port machine in `raven-rp1-xhci` remains the controller-side model; this module
//! is the class/port view above it and is deliberately independent of the R4 API.

use alloc::vec::Vec;
use core::fmt;

/// Largest number of ports a hub model holds.
pub const MAX_HUB_PORTS: usize = 15;

/// The negotiated port speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortSpeed {
    /// USB 1.1 low speed.
    Low,
    /// USB 1.1 full speed.
    Full,
    /// USB 2.0 high speed.
    High,
    /// USB 3.x SuperSpeed.
    Super,
}

/// The port state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubPortState {
    /// Nothing is attached.
    Disconnected,
    /// A device is attached but the port is not powered.
    Attached,
    /// The port is powered and waiting for a reset.
    Powered,
    /// A reset is in progress.
    Resetting,
    /// The device is enabled and can be enumerated.
    Enabled,
    /// The port is suspended.
    Suspended,
    /// An over-current condition is active.
    OverCurrent,
}

/// The change bits raised by port events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortChange {
    /// The connect state changed.
    pub connect: bool,
    /// The enable state changed.
    pub enable: bool,
    /// A reset completed or started.
    pub reset: bool,
    /// The suspend/resume state changed.
    pub suspend: bool,
    /// An over-current condition changed.
    pub over_current: bool,
}

impl PortChange {
    /// No change.
    pub const NONE: Self = Self {
        connect: false,
        enable: false,
        reset: false,
        suspend: false,
        over_current: false,
    };

    /// True when any change bit is set.
    #[must_use]
    pub const fn any(self) -> bool {
        self.connect || self.enable || self.reset || self.suspend || self.over_current
    }
}

/// What happened to a port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubEvent {
    /// A device attached at `speed`.
    Attach(PortSpeed),
    /// The port power is on.
    PowerOn,
    /// Start a reset.
    Reset,
    /// The reset completed.
    ResetComplete,
    /// Suspend the port.
    Suspend,
    /// Resume the port.
    Resume,
    /// The device detached.
    Detach,
    /// Over-current detected.
    OverCurrent,
    /// The over-current condition cleared.
    ClearOverCurrent,
    /// The enumeration assigned `address`.
    Address(u8),
    /// The device selected `configuration`.
    Configure(u8),
}

/// Why a port transition failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubError {
    /// The port count is zero or above [`MAX_HUB_PORTS`].
    PortCount(u8),
    /// The port index is outside the hub.
    PortOutOfRange(u8),
    /// The event does not belong to the current state.
    UnexpectedEvent {
        /// Current state.
        state: HubPortState,
        /// Rejected event.
        event: HubEvent,
    },
    /// The address is zero or above 127.
    InvalidAddress(u8),
    /// The configuration value is zero.
    InvalidConfiguration(u8),
}

impl fmt::Display for HubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PortCount(ports) => {
                write!(f, "hub port count {ports} is outside 1..={MAX_HUB_PORTS}")
            }
            Self::PortOutOfRange(port) => write!(f, "hub port {port} is outside the hub"),
            Self::UnexpectedEvent { state, event } => {
                write!(f, "event {event:?} is not valid while {state:?}")
            }
            Self::InvalidAddress(address) => write!(f, "USB address {address} is invalid"),
            Self::InvalidConfiguration(value) => {
                write!(f, "configuration value {value} is invalid")
            }
        }
    }
}

impl core::error::Error for HubError {}

/// One hub port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HubPort {
    index: u8,
    state: HubPortState,
    speed: Option<PortSpeed>,
    address: Option<u8>,
    configuration: Option<u8>,
    changes: PortChange,
}

impl HubPort {
    const fn new(index: u8) -> Self {
        Self {
            index,
            state: HubPortState::Disconnected,
            speed: None,
            address: None,
            configuration: None,
            changes: PortChange::NONE,
        }
    }

    /// Port index (zero-based).
    #[must_use]
    pub const fn index(self) -> u8 {
        self.index
    }

    /// Current state.
    #[must_use]
    pub const fn state(self) -> HubPortState {
        self.state
    }

    /// Attached speed, if any.
    #[must_use]
    pub const fn speed(self) -> Option<PortSpeed> {
        self.speed
    }

    /// Assigned USB address, if enumeration reached `SET_ADDRESS`.
    #[must_use]
    pub const fn address(self) -> Option<u8> {
        self.address
    }

    /// Selected configuration, if enumeration reached `SET_CONFIGURATION`.
    #[must_use]
    pub const fn configuration(self) -> Option<u8> {
        self.configuration
    }

    /// Change bits since the last acknowledge.
    #[must_use]
    pub const fn changes(self) -> PortChange {
        self.changes
    }

    /// Clear the change bits.
    pub fn acknowledge(&mut self) {
        self.changes = PortChange::NONE;
    }

    fn unexpected(self, event: HubEvent) -> HubError {
        HubError::UnexpectedEvent {
            state: self.state,
            event,
        }
    }

    /// Apply an event.
    ///
    /// # Errors
    ///
    /// Returns [`HubError::UnexpectedEvent`] when the event does not belong to the current state;
    /// the state is unchanged.
    pub fn event(&mut self, event: HubEvent) -> Result<(), HubError> {
        match event {
            HubEvent::Attach(speed) => {
                if self.state != HubPortState::Disconnected {
                    return Err(self.unexpected(event));
                }
                self.state = HubPortState::Attached;
                self.speed = Some(speed);
                self.address = None;
                self.configuration = None;
                self.changes.connect = true;
            }
            HubEvent::PowerOn => {
                if self.state != HubPortState::Attached {
                    return Err(self.unexpected(event));
                }
                self.state = HubPortState::Powered;
            }
            HubEvent::Reset => {
                if !matches!(self.state, HubPortState::Powered | HubPortState::Enabled) {
                    return Err(self.unexpected(event));
                }
                self.state = HubPortState::Resetting;
                self.address = None;
                self.configuration = None;
                self.changes.reset = true;
            }
            HubEvent::ResetComplete => {
                if self.state != HubPortState::Resetting {
                    return Err(self.unexpected(event));
                }
                self.state = HubPortState::Enabled;
                self.changes.enable = true;
                self.changes.reset = true;
            }
            HubEvent::Suspend => {
                if self.state != HubPortState::Enabled {
                    return Err(self.unexpected(event));
                }
                self.state = HubPortState::Suspended;
                self.changes.suspend = true;
            }
            HubEvent::Resume => {
                if self.state != HubPortState::Suspended {
                    return Err(self.unexpected(event));
                }
                self.state = HubPortState::Enabled;
                self.changes.suspend = true;
            }
            HubEvent::Detach => {
                if self.state == HubPortState::Disconnected {
                    return Err(self.unexpected(event));
                }
                self.state = HubPortState::Disconnected;
                self.speed = None;
                self.address = None;
                self.configuration = None;
                self.changes.connect = true;
                self.changes.enable = true;
                self.changes.suspend = true;
            }
            HubEvent::OverCurrent => {
                if self.state == HubPortState::OverCurrent {
                    return Err(self.unexpected(event));
                }
                self.state = HubPortState::OverCurrent;
                self.address = None;
                self.configuration = None;
                self.changes.over_current = true;
            }
            HubEvent::ClearOverCurrent => {
                if self.state != HubPortState::OverCurrent {
                    return Err(self.unexpected(event));
                }
                self.state = HubPortState::Disconnected;
                self.speed = None;
                self.changes.over_current = true;
            }
            HubEvent::Address(address) => {
                if self.state != HubPortState::Enabled {
                    return Err(self.unexpected(event));
                }
                if address == 0 || address > 127 {
                    return Err(HubError::InvalidAddress(address));
                }
                self.address = Some(address);
            }
            HubEvent::Configure(value) => {
                if self.state != HubPortState::Enabled || self.address.is_none() {
                    return Err(self.unexpected(event));
                }
                if value == 0 {
                    return Err(HubError::InvalidConfiguration(value));
                }
                self.configuration = Some(value);
            }
        }
        Ok(())
    }
}

/// A hub with downstream ports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hub {
    ports: Vec<HubPort>,
}

impl Hub {
    /// Build a hub with `ports` ports.
    ///
    /// # Errors
    ///
    /// Returns [`HubError::PortCount`] for zero or more than [`MAX_HUB_PORTS`].
    pub fn new(ports: u8) -> Result<Self, HubError> {
        if ports == 0 || usize::from(ports) > MAX_HUB_PORTS {
            return Err(HubError::PortCount(ports));
        }
        Ok(Self {
            ports: (0..ports).map(HubPort::new).collect(),
        })
    }

    /// Number of ports.
    #[must_use]
    pub fn port_count(&self) -> usize {
        self.ports.len()
    }

    /// One port.
    #[must_use]
    pub fn port(&self, index: u8) -> Option<&HubPort> {
        self.ports.get(usize::from(index))
    }

    /// Mutable one port.
    #[must_use]
    pub fn port_mut(&mut self, index: u8) -> Option<&mut HubPort> {
        self.ports.get_mut(usize::from(index))
    }

    /// Apply an event to a port.
    ///
    /// # Errors
    ///
    /// Returns [`HubError::PortOutOfRange`] or the port transition error.
    pub fn event(&mut self, index: u8, event: HubEvent) -> Result<(), HubError> {
        let Some(port) = self.ports.get_mut(usize::from(index)) else {
            return Err(HubError::PortOutOfRange(index));
        };
        port.event(event)
    }

    /// Whether any port has a change bit.
    #[must_use]
    pub fn any_change(&self) -> bool {
        self.ports.iter().any(|port| port.changes.any())
    }

    /// Enabled port count.
    #[must_use]
    pub fn enabled(&self) -> usize {
        self.ports
            .iter()
            .filter(|port| port.state == HubPortState::Enabled)
            .count()
    }

    /// Acknowledge every port's change bits.
    pub fn acknowledge_all(&mut self) {
        for port in &mut self.ports {
            port.acknowledge();
        }
    }
}

#[cfg(test)]
#[path = "hub_ut.rs"]
mod tests;
