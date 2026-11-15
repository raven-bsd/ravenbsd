// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MDIO command encoding and the bounded busy-poll transaction state machine.
//!
//! The GENET MDIO register carries the PHY address, register number, operation and data in one
//! 32-bit command. A transaction is started by setting `START_BUSY`; software polls the same
//! register until the bit clears. The Pi 4 driver uses a 10 us interval and a 1000-poll budget,
//! so a stuck transaction fails after 10 ms instead of spinning forever.

use crate::regs::{
    MDIO_DATA_MASK, MDIO_PMD_MASK, MDIO_PMD_SHIFT, MDIO_READ, MDIO_REG_MASK, MDIO_REG_SHIFT,
    MDIO_START_BUSY, MDIO_WRITE,
};

/// Polls allowed for one transaction.
pub const MDIO_BUSY_POLLS: u16 = 1000;
/// Delay between two polls, in microseconds.
pub const MDIO_POLL_INTERVAL_US: u32 = 10;
/// Maximum time one transaction may take, in microseconds.
pub const MDIO_MAX_WAIT_US: u32 = MDIO_BUSY_POLLS as u32 * MDIO_POLL_INTERVAL_US;
/// Largest PHY address the command field can carry.
pub const MAX_PHY_ADDRESS: u8 = 31;
/// Largest MDIO register number the command field can carry.
pub const MAX_MDIO_REGISTER: u8 = 31;

/// Why an MDIO command could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdioError {
    /// The PHY address does not fit the five-bit field.
    PhyAddress(u8),
    /// The register number does not fit the five-bit field.
    Register(u8),
}

/// One MDIO read or write request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MdioCommand {
    phy: u8,
    register: u8,
    write: Option<u16>,
}

impl MdioCommand {
    /// Build a read request.
    pub fn read(phy: u8, register: u8) -> Result<Self, MdioError> {
        validate(phy, register)?;
        Ok(Self {
            phy,
            register,
            write: None,
        })
    }

    /// Build a write request.
    pub fn write(phy: u8, register: u8, value: u16) -> Result<Self, MdioError> {
        validate(phy, register)?;
        Ok(Self {
            phy,
            register,
            write: Some(value),
        })
    }

    /// The PHY address.
    #[must_use]
    pub const fn phy(self) -> u8 {
        self.phy
    }

    /// The register number.
    #[must_use]
    pub const fn register(self) -> u8 {
        self.register
    }

    /// True for a read request.
    #[must_use]
    pub const fn is_read(self) -> bool {
        self.write.is_none()
    }

    /// True for a write request.
    #[must_use]
    pub const fn is_write(self) -> bool {
        self.write.is_some()
    }

    /// The write value, when this is a write request.
    #[must_use]
    pub const fn write_value(self) -> Option<u16> {
        self.write
    }

    /// Encode the register value written to start the transaction.
    #[must_use]
    pub const fn register_value(self) -> u32 {
        let operation = match self.write {
            Some(_) => MDIO_WRITE,
            None => MDIO_READ,
        };
        let data = match self.write {
            Some(value) => value as u32,
            None => 0,
        };
        MDIO_START_BUSY
            | operation
            | (((self.phy as u32) << MDIO_PMD_SHIFT) & MDIO_PMD_MASK)
            | (((self.register as u32) << MDIO_REG_SHIFT) & MDIO_REG_MASK)
            | (data & MDIO_DATA_MASK)
    }
}

fn validate(phy: u8, register: u8) -> Result<(), MdioError> {
    if phy > MAX_PHY_ADDRESS {
        return Err(MdioError::PhyAddress(phy));
    }
    if register > MAX_MDIO_REGISTER {
        return Err(MdioError::Register(register));
    }
    Ok(())
}

/// The result of observing the MDIO command register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdioState {
    /// The transaction is still busy.
    Waiting,
    /// A read completed with this value.
    ReadComplete(u16),
    /// A write completed.
    WriteComplete,
    /// The poll budget ran out.
    TimedOut,
}

/// One in-flight MDIO transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MdioTransaction {
    command: MdioCommand,
    state: MdioState,
    polls_left: u16,
}

impl MdioTransaction {
    /// Start a read transaction.
    pub fn start_read(phy: u8, register: u8) -> Result<Self, MdioError> {
        Ok(Self {
            command: MdioCommand::read(phy, register)?,
            state: MdioState::Waiting,
            polls_left: MDIO_BUSY_POLLS,
        })
    }

    /// Start a write transaction.
    pub fn start_write(phy: u8, register: u8, value: u16) -> Result<Self, MdioError> {
        Ok(Self {
            command: MdioCommand::write(phy, register, value)?,
            state: MdioState::Waiting,
            polls_left: MDIO_BUSY_POLLS,
        })
    }

    /// The command to write to `MDIO_CMD`.
    #[must_use]
    pub const fn command(self) -> MdioCommand {
        self.command
    }

    /// The current state.
    #[must_use]
    pub const fn state(self) -> MdioState {
        self.state
    }

    /// Polls still available before a timeout.
    #[must_use]
    pub const fn polls_left(self) -> u16 {
        self.polls_left
    }

    /// Observe one read of the command register.
    ///
    /// The first observation may already be complete. While `START_BUSY` stays set, the budget
    /// is consumed; when it is exhausted the state becomes [`MdioState::TimedOut`].
    pub fn observe(&mut self, command_register: u32) -> MdioState {
        if self.state != MdioState::Waiting {
            return self.state;
        }
        if command_register & MDIO_START_BUSY == 0 {
            self.state = match self.command.write {
                Some(_) => MdioState::WriteComplete,
                None => MdioState::ReadComplete((command_register & MDIO_DATA_MASK) as u16),
            };
            return self.state;
        }
        if self.polls_left == 0 {
            self.state = MdioState::TimedOut;
            return self.state;
        }
        self.polls_left -= 1;
        self.state
    }
}

#[cfg(test)]
#[path = "mdio_ut.rs"]
mod tests;
