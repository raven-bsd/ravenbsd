// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Power-domain dependency model.
//!
//! The Pi 5 firmware tree has no RP1 power-controller register block: the RP1 firmware owns its
//! power and reset state, and the exposed `power-domains` phandles on RP1 children point at the
//! BCM2712 PM block, not at RP1. This module therefore models only the invariants a bring-up needs
//! before it can touch any register: a domain may be powered on only after its parent, a domain
//! with users or enabled children cannot be powered off, and a reset is issued only while the
//! domain is off. None of it is a hardware claim.

use core::fmt;

/// Largest number of domains the model holds.
pub const MAX_DOMAINS: usize = 16;

/// A domain and its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainSpec {
    /// Domain ID.
    pub id: u16,
    /// Domain that must be on first, when known.
    pub parent: Option<u16>,
}

/// Whether a domain is powered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainState {
    /// Powered off.
    Off,
    /// Powered on.
    On,
}

/// The state of one domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Domain {
    /// The static description.
    pub spec: DomainSpec,
    /// Current state.
    pub state: DomainState,
    /// Number of clients that acquired the domain.
    pub users: u16,
}

/// The observable result of a transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    /// The domain was already on.
    AlreadyOn(u16),
    /// The domain turned on.
    PoweredOn(u16),
    /// The domain was already off.
    AlreadyOff(u16),
    /// The domain turned off.
    PoweredOff(u16),
    /// A reset pulse is allowed.
    Reset(u16),
}

/// Why a power transition is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerError {
    /// The domain is not in the model.
    Unknown(u16),
    /// The parent domain is not in the model.
    UnknownParent(u16),
    /// The parent must be powered on first.
    ParentOff {
        /// Domain that was requested.
        id: u16,
        /// Parent that is off.
        parent: u16,
    },
    /// Clients still hold the domain.
    InUse {
        /// Domain that was requested.
        id: u16,
        /// Remaining users.
        users: u16,
    },
    /// A child domain is still on.
    ChildOn {
        /// Domain that was requested.
        id: u16,
        /// Child that is on.
        child: u16,
    },
    /// A client tried to use a domain that is off.
    NotOn(u16),
    /// A release arrived with no matching acquire.
    UsersUnderflow(u16),
    /// A reset was requested while the domain is on.
    ResetWhileOn(u16),
    /// The model is full or the ID is already present.
    Duplicate(u16),
    /// The model has no free slot.
    Full,
}

impl fmt::Display for PowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(id) => write!(f, "power domain {id} is not in the model"),
            Self::UnknownParent(parent) => write!(f, "parent domain {parent} is not in the model"),
            Self::ParentOff { id, parent } => {
                write!(f, "domain {id} needs parent {parent} powered on first")
            }
            Self::InUse { id, users } => {
                write!(f, "domain {id} still has {users} user(s)")
            }
            Self::ChildOn { id, child } => {
                write!(f, "domain {id} still powers child {child}")
            }
            Self::NotOn(id) => write!(f, "domain {id} is not powered"),
            Self::UsersUnderflow(id) => write!(f, "domain {id} has no user to release"),
            Self::ResetWhileOn(id) => write!(f, "domain {id} must be off before a reset"),
            Self::Duplicate(id) => write!(f, "domain {id} is already present"),
            Self::Full => f.write_str("power-domain model is full"),
        }
    }
}

impl core::error::Error for PowerError {}

/// A fixed-capacity set of power domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerModel {
    domains: [Option<Domain>; MAX_DOMAINS],
    count: usize,
}

impl PowerModel {
    /// An empty model; every domain starts off.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            domains: [None; MAX_DOMAINS],
            count: 0,
        }
    }

    /// Add a domain.
    ///
    /// # Errors
    ///
    /// Returns [`PowerError::Duplicate`] for a repeated ID and [`PowerError::Full`] when the
    /// model has no slot left.
    pub fn insert(&mut self, spec: DomainSpec) -> Result<(), PowerError> {
        if self.get(spec.id).is_some() {
            return Err(PowerError::Duplicate(spec.id));
        }
        match self.domains.get_mut(self.count) {
            Some(slot) => {
                *slot = Some(Domain {
                    spec,
                    state: DomainState::Off,
                    users: 0,
                });
                self.count = self.count.saturating_add(1);
                Ok(())
            }
            None => Err(PowerError::Full),
        }
    }

    /// Number of domains.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// True when the model is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Look up a domain.
    #[must_use]
    pub fn get(&self, id: u16) -> Option<&Domain> {
        self.domains
            .iter()
            .flatten()
            .find(|domain| domain.spec.id == id)
    }

    /// The state of a domain.
    #[must_use]
    pub fn state(&self, id: u16) -> Option<DomainState> {
        self.get(id).map(|domain| domain.state)
    }

    /// Power a domain on; its parent must already be on.
    ///
    /// # Errors
    ///
    /// Returns [`PowerError::Unknown`], [`PowerError::UnknownParent`] or
    /// [`PowerError::ParentOff`].
    pub fn power_on(&mut self, id: u16) -> Result<PowerAction, PowerError> {
        let domain = *self.get(id).ok_or(PowerError::Unknown(id))?;
        if domain.state == DomainState::On {
            return Ok(PowerAction::AlreadyOn(id));
        }
        if let Some(parent_id) = domain.spec.parent {
            let parent = self
                .get(parent_id)
                .ok_or(PowerError::UnknownParent(parent_id))?;
            if parent.state != DomainState::On {
                return Err(PowerError::ParentOff {
                    id,
                    parent: parent_id,
                });
            }
        }
        self.set_state(id, DomainState::On);
        Ok(PowerAction::PoweredOn(id))
    }

    /// Power a domain off; it must have no users and no powered child.
    ///
    /// # Errors
    ///
    /// Returns [`PowerError::Unknown`], [`PowerError::InUse`] or [`PowerError::ChildOn`].
    pub fn power_off(&mut self, id: u16) -> Result<PowerAction, PowerError> {
        let domain = *self.get(id).ok_or(PowerError::Unknown(id))?;
        if domain.users != 0 {
            return Err(PowerError::InUse {
                id,
                users: domain.users,
            });
        }
        if let Some(child) = self
            .domains
            .iter()
            .flatten()
            .find(|child| child.spec.parent == Some(id) && child.state == DomainState::On)
        {
            return Err(PowerError::ChildOn {
                id,
                child: child.spec.id,
            });
        }
        if domain.state == DomainState::Off {
            return Ok(PowerAction::AlreadyOff(id));
        }
        self.set_state(id, DomainState::Off);
        Ok(PowerAction::PoweredOff(id))
    }

    /// Acquire a client reference; the domain must be on.
    ///
    /// # Errors
    ///
    /// Returns [`PowerError::Unknown`] or [`PowerError::NotOn`].
    pub fn acquire(&mut self, id: u16) -> Result<(), PowerError> {
        let domain = *self.get(id).ok_or(PowerError::Unknown(id))?;
        if domain.state != DomainState::On {
            return Err(PowerError::NotOn(id));
        }
        if let Some(slot) = self
            .domains
            .iter_mut()
            .take(self.count)
            .flatten()
            .find(|slot| slot.spec.id == id)
        {
            slot.users = slot.users.saturating_add(1);
        }
        Ok(())
    }

    /// Release a client reference.
    ///
    /// # Errors
    ///
    /// Returns [`PowerError::Unknown`] or [`PowerError::UsersUnderflow`].
    pub fn release(&mut self, id: u16) -> Result<(), PowerError> {
        let domain = *self.get(id).ok_or(PowerError::Unknown(id))?;
        if domain.users == 0 {
            return Err(PowerError::UsersUnderflow(id));
        }
        if let Some(slot) = self
            .domains
            .iter_mut()
            .take(self.count)
            .flatten()
            .find(|slot| slot.spec.id == id)
        {
            slot.users = slot.users.saturating_sub(1);
        }
        Ok(())
    }

    /// Allow a reset pulse; the domain must be off and unused.
    ///
    /// # Errors
    ///
    /// Returns [`PowerError::Unknown`], [`PowerError::InUse`] or [`PowerError::ResetWhileOn`].
    pub fn reset(&mut self, id: u16) -> Result<PowerAction, PowerError> {
        let domain = *self.get(id).ok_or(PowerError::Unknown(id))?;
        if domain.users != 0 {
            return Err(PowerError::InUse {
                id,
                users: domain.users,
            });
        }
        if domain.state != DomainState::Off {
            return Err(PowerError::ResetWhileOn(id));
        }
        Ok(PowerAction::Reset(id))
    }

    fn set_state(&mut self, id: u16, state: DomainState) {
        if let Some(slot) = self
            .domains
            .iter_mut()
            .take(self.count)
            .flatten()
            .find(|slot| slot.spec.id == id)
        {
            slot.state = state;
        }
    }
}

impl Default for PowerModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "power_ut.rs"]
mod tests;
