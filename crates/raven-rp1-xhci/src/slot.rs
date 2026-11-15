// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! xHCI slots and endpoints.
//!
//! A slot moves `Disabled -> Enabled -> Addressed -> Configured` through the Enable Slot,
//! Address Device and Configure Endpoint commands. Endpoint 0 (device context index 1) becomes
//! running when the slot is addressed; other endpoints start, halt, stop and reset through the
//! standard endpoint commands. The model validates the slot/endpoint ranges against the
//! controller's capability and never lets an endpoint leave a disabled slot.

use core::fmt;

/// Largest number of slots the model holds.
pub const MAX_SLOTS: usize = 64;
/// Number of device context indexes (EP0 plus endpoints 1..=30).
pub const MAX_ENDPOINTS: usize = 32;

/// The slot state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    /// The slot is free.
    Disabled,
    /// The Enable Slot command completed.
    Enabled,
    /// Address Device completed.
    Addressed,
    /// Configure Endpoint completed.
    Configured,
}

/// The endpoint state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointState {
    /// Not configured.
    Disabled,
    /// Running.
    Running,
    /// Halted after a transfer error.
    Halted,
    /// Stopped by the host.
    Stopped,
    /// A fatal endpoint error.
    Error,
}

/// One slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// Slot ID, `1..=max_slots`.
    pub id: u8,
    /// Slot state.
    pub state: SlotState,
    endpoints: [EndpointState; MAX_ENDPOINTS],
}

impl Slot {
    fn new(id: u8) -> Self {
        Self {
            id,
            state: SlotState::Enabled,
            endpoints: [EndpointState::Disabled; MAX_ENDPOINTS],
        }
    }

    /// Endpoint state for a device context index (`1..=31`).
    #[must_use]
    pub fn endpoint(&self, dci: u8) -> Option<EndpointState> {
        if dci == 0 || dci as usize >= MAX_ENDPOINTS {
            return None;
        }
        self.endpoints.get(usize::from(dci)).copied()
    }

    fn endpoint_mut(&mut self, dci: u8) -> Option<&mut EndpointState> {
        if dci == 0 || dci as usize >= MAX_ENDPOINTS {
            return None;
        }
        self.endpoints.get_mut(usize::from(dci))
    }
}

/// Why a slot or endpoint transition is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotTableError {
    /// The maximum slot count is zero or above [`MAX_SLOTS`].
    MaxSlots(u8),
    /// The slot is not enabled.
    UnknownSlot(u8),
    /// The slot is in the wrong state for the command.
    BadState {
        /// Current state.
        state: SlotState,
        /// State the command needs.
        expected: SlotState,
    },
    /// The device context index is outside `1..=31`.
    BadEndpoint(u8),
    /// The endpoint transition is invalid.
    BadEndpointState {
        /// Current endpoint state.
        state: EndpointState,
        /// The requested transition.
        transition: &'static str,
    },
    /// The table has no free slot.
    Full,
}

impl fmt::Display for SlotTableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MaxSlots(slots) => write!(f, "max slots {slots} is outside 1..={MAX_SLOTS}"),
            Self::UnknownSlot(slot) => write!(f, "slot {slot} is not enabled"),
            Self::BadState { state, expected } => {
                write!(f, "slot is {state:?}, expected {expected:?}")
            }
            Self::BadEndpoint(dci) => write!(f, "endpoint {dci} is outside 1..=31"),
            Self::BadEndpointState { state, transition } => {
                write!(f, "endpoint is {state:?}; cannot {transition}")
            }
            Self::Full => f.write_str("no free slot"),
        }
    }
}

impl core::error::Error for SlotTableError {}

/// The slot table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotTable {
    max_slots: u8,
    slots: [Option<Slot>; MAX_SLOTS],
}

impl SlotTable {
    /// Build a table for `max_slots` slots (from `HCSPARAMS1`).
    ///
    /// # Errors
    ///
    /// Returns [`SlotTableError::MaxSlots`] for zero slots or more than [`MAX_SLOTS`].
    pub const fn new(max_slots: u8) -> Result<Self, SlotTableError> {
        if max_slots == 0 || max_slots as usize > MAX_SLOTS {
            return Err(SlotTableError::MaxSlots(max_slots));
        }
        Ok(Self {
            max_slots,
            slots: [None; MAX_SLOTS],
        })
    }

    /// The configured slot count.
    #[must_use]
    pub const fn max_slots(&self) -> u8 {
        self.max_slots
    }

    /// The slot with `id`, when enabled.
    #[must_use]
    pub fn slot(&self, id: u8) -> Option<&Slot> {
        if id == 0 || id > self.max_slots {
            return None;
        }
        self.slots.get(usize::from(id)).and_then(Option::as_ref)
    }

    /// The state of a slot.
    #[must_use]
    pub fn state(&self, id: u8) -> Option<SlotState> {
        self.slot(id).map(|slot| slot.state)
    }

    /// Enable the lowest free slot, as the Enable Slot command does.
    ///
    /// # Errors
    ///
    /// Returns [`SlotTableError::Full`] when every slot is in use.
    pub fn enable(&mut self) -> Result<u8, SlotTableError> {
        for id in 1..=self.max_slots {
            let index = usize::from(id);
            if self.slots.get(index).is_some_and(Option::is_none) {
                if let Some(slot) = self.slots.get_mut(index) {
                    *slot = Some(Slot::new(id));
                }
                return Ok(id);
            }
        }
        Err(SlotTableError::Full)
    }

    /// Address a slot and start endpoint 0.
    ///
    /// # Errors
    ///
    /// Returns [`SlotTableError::UnknownSlot`] or [`SlotTableError::BadState`].
    pub fn address(&mut self, id: u8) -> Result<(), SlotTableError> {
        self.require(id, SlotState::Enabled)?;
        let Some(slot) = self.slots.get_mut(usize::from(id)).and_then(Option::as_mut) else {
            return Err(SlotTableError::UnknownSlot(id));
        };
        slot.state = SlotState::Addressed;
        if let Some(endpoint) = slot.endpoint_mut(1) {
            *endpoint = EndpointState::Running;
        }
        Ok(())
    }

    /// Configure a slot.
    ///
    /// # Errors
    ///
    /// Returns [`SlotTableError::UnknownSlot`] or [`SlotTableError::BadState`].
    pub fn configure(&mut self, id: u8) -> Result<(), SlotTableError> {
        self.require(id, SlotState::Addressed)?;
        let Some(slot) = self.slots.get_mut(usize::from(id)).and_then(Option::as_mut) else {
            return Err(SlotTableError::UnknownSlot(id));
        };
        slot.state = SlotState::Configured;
        Ok(())
    }

    /// Disable a slot and release it.
    ///
    /// # Errors
    ///
    /// Returns [`SlotTableError::UnknownSlot`].
    pub fn disable(&mut self, id: u8) -> Result<(), SlotTableError> {
        if self.slot(id).is_none() {
            return Err(SlotTableError::UnknownSlot(id));
        }
        if let Some(slot) = self.slots.get_mut(usize::from(id)) {
            *slot = None;
        }
        Ok(())
    }

    /// The endpoint state for `(slot, dci)`.
    #[must_use]
    pub fn endpoint(&self, id: u8, dci: u8) -> Option<EndpointState> {
        self.slot(id)?.endpoint(dci)
    }

    /// Enable an endpoint after Configure Endpoint (a disabled endpoint becomes running).
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown or unaddressed slot, a bad DCI or an endpoint that is not
    /// disabled.
    pub fn endpoint_enable(&mut self, id: u8, dci: u8) -> Result<(), SlotTableError> {
        let slot = self.slot(id).ok_or(SlotTableError::UnknownSlot(id))?;
        if !matches!(slot.state, SlotState::Addressed | SlotState::Configured) {
            return Err(SlotTableError::BadState {
                state: slot.state,
                expected: SlotState::Addressed,
            });
        }
        self.transition(id, dci, "enable", |state| match state {
            EndpointState::Disabled => Some(EndpointState::Running),
            _ => None,
        })
    }

    /// Start a stopped endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown slot, a bad DCI or an invalid transition.
    pub fn endpoint_start(&mut self, id: u8, dci: u8) -> Result<(), SlotTableError> {
        self.transition(id, dci, "start", |state| match state {
            EndpointState::Stopped => Some(EndpointState::Running),
            _ => None,
        })
    }

    /// Halt a running endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown slot, a bad DCI or an invalid transition.
    pub fn endpoint_halt(&mut self, id: u8, dci: u8) -> Result<(), SlotTableError> {
        self.transition(id, dci, "halt", |state| match state {
            EndpointState::Running => Some(EndpointState::Halted),
            _ => None,
        })
    }

    /// Stop a running endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown slot, a bad DCI or an invalid transition.
    pub fn endpoint_stop(&mut self, id: u8, dci: u8) -> Result<(), SlotTableError> {
        self.transition(id, dci, "stop", |state| match state {
            EndpointState::Running => Some(EndpointState::Stopped),
            _ => None,
        })
    }

    /// Reset a halted endpoint back to running.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown slot, a bad DCI or an invalid transition.
    pub fn endpoint_reset(&mut self, id: u8, dci: u8) -> Result<(), SlotTableError> {
        self.transition(id, dci, "reset", |state| match state {
            EndpointState::Halted => Some(EndpointState::Running),
            _ => None,
        })
    }

    /// Mark an endpoint failed.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown slot or a bad DCI.
    pub fn endpoint_error(&mut self, id: u8, dci: u8) -> Result<(), SlotTableError> {
        self.transition(id, dci, "fail", |_| Some(EndpointState::Error))
    }

    fn require(&self, id: u8, expected: SlotState) -> Result<(), SlotTableError> {
        let slot = self.slot(id).ok_or(SlotTableError::UnknownSlot(id))?;
        if slot.state != expected {
            return Err(SlotTableError::BadState {
                state: slot.state,
                expected,
            });
        }
        Ok(())
    }

    fn transition(
        &mut self,
        id: u8,
        dci: u8,
        name: &'static str,
        rule: impl Fn(EndpointState) -> Option<EndpointState>,
    ) -> Result<(), SlotTableError> {
        let slot = self
            .slots
            .get_mut(usize::from(id))
            .and_then(Option::as_mut)
            .ok_or(SlotTableError::UnknownSlot(id))?;
        if slot.state == SlotState::Disabled {
            return Err(SlotTableError::UnknownSlot(id));
        }
        let endpoint = slot
            .endpoint_mut(dci)
            .ok_or(SlotTableError::BadEndpoint(dci))?;
        match rule(*endpoint) {
            Some(next) => {
                *endpoint = next;
                Ok(())
            }
            None => Err(SlotTableError::BadEndpointState {
                state: *endpoint,
                transition: name,
            }),
        }
    }
}

#[cfg(test)]
#[path = "slot_ut.rs"]
mod tests;
