// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MDIO clause 22 frames and the PHY transaction state machine.
//!
//! The IEEE 802.3 clause 22 frame is standard: 32 preamble bits, start `01`, opcode (`10` read,
//! `01` write), 5-bit PHY address, 5-bit register address, 2-bit turnaround (`10`) and 16 data
//! bits. The model works on the 32-bit word after the preamble; the RP1 MDIO clock divider and
//! the PHY-specific registers are UNVERIFIED.

use core::fmt;

/// The BMCR register (standard clause 22 register 0).
pub const BMCR: u8 = 0;
/// The BMSR register (standard clause 22 register 1).
pub const BMSR: u8 = 1;

/// Start-of-frame field.
pub const START: u32 = 0b01;
/// Read opcode.
pub const OP_READ: u32 = 0b10;
/// Write opcode.
pub const OP_WRITE: u32 = 0b01;

/// Why an MDIO transaction failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdioError {
    /// The PHY address does not fit five bits.
    BadPhy(u8),
    /// The register address does not fit five bits.
    BadRegister(u8),
    /// The response start field is not `01`.
    BadStart,
    /// The response opcode is not a read.
    BadOpcode,
    /// The turnaround field is not `10`.
    BadTurnaround,
    /// The transaction timed out after its retry budget.
    Timeout,
    /// A transaction is already in flight.
    Busy,
    /// The machine faulted; it must be reset before a new transaction.
    Faulted,
    /// No transaction is in flight.
    Idle,
}

impl fmt::Display for MdioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadPhy(phy) => write!(f, "PHY address {phy} does not fit five bits"),
            Self::BadRegister(register) => {
                write!(f, "MDIO register {register} does not fit five bits")
            }
            Self::BadStart => f.write_str("MDIO response start field is not 01"),
            Self::BadOpcode => f.write_str("MDIO response opcode is not a read"),
            Self::BadTurnaround => f.write_str("MDIO response turnaround is not 10"),
            Self::Timeout => f.write_str("MDIO transaction timed out"),
            Self::Busy => f.write_str("an MDIO transaction is already in flight"),
            Self::Faulted => f.write_str("MDIO machine is faulted; reset it first"),
            Self::Idle => f.write_str("no MDIO transaction is in flight"),
        }
    }
}

impl core::error::Error for MdioError {}

/// A clause 22 read or write request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transaction {
    /// Read a register.
    Read {
        /// PHY address, `0..=31`.
        phy: u8,
        /// Register address, `0..=31`.
        register: u8,
    },
    /// Write a register.
    Write {
        /// PHY address, `0..=31`.
        phy: u8,
        /// Register address, `0..=31`.
        register: u8,
        /// Value to write.
        value: u16,
    },
}

/// An address pair for a clause 22 access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clause22 {
    /// PHY address.
    pub phy: u8,
    /// Register address.
    pub register: u8,
}

impl Clause22 {
    /// Validate the address pair.
    ///
    /// # Errors
    ///
    /// Returns [`MdioError::BadPhy`] or [`MdioError::BadRegister`] above 31.
    pub const fn new(phy: u8, register: u8) -> Result<Self, MdioError> {
        if phy > 31 {
            return Err(MdioError::BadPhy(phy));
        }
        if register > 31 {
            return Err(MdioError::BadRegister(register));
        }
        Ok(Self { phy, register })
    }
}

const fn header(opcode: u32, pair: Clause22) -> u32 {
    (START << 30)
        | (opcode << 28)
        | ((pair.phy as u32) << 23)
        | ((pair.register as u32) << 18)
        | (0b10 << 16)
}

/// Encode a clause 22 read command (the 32-bit word after the preamble).
///
/// # Errors
///
/// Returns [`MdioError`] for an out-of-range address pair.
pub fn read_command(phy: u8, register: u8) -> Result<u32, MdioError> {
    Ok(header(OP_READ, Clause22::new(phy, register)?))
}

/// Encode a clause 22 write command and its data.
///
/// # Errors
///
/// Returns [`MdioError`] for an out-of-range address pair.
pub fn write_command(phy: u8, register: u8, value: u16) -> Result<(u32, u16), MdioError> {
    Ok((header(OP_WRITE, Clause22::new(phy, register)?), value))
}

/// Decode a clause 22 read response.
///
/// # Errors
///
/// Returns [`MdioError`] when the start, opcode or turnaround fields are wrong.
pub fn decode_read(word: u32) -> Result<u16, MdioError> {
    if (word >> 30) & 0b11 != START {
        return Err(MdioError::BadStart);
    }
    if (word >> 28) & 0b11 != OP_READ {
        return Err(MdioError::BadOpcode);
    }
    if (word >> 16) & 0b11 != 0b10 {
        return Err(MdioError::BadTurnaround);
    }
    Ok(word as u16)
}

/// The standard BMSR/BMCR link fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhyStatus {
    /// BMSR bit 2: link is up.
    pub link_up: bool,
    /// BMSR bit 5: autonegotiation completed.
    pub autoneg_complete: bool,
    /// BMCR bit 12: autonegotiation enabled.
    pub autoneg_enabled: bool,
    /// BMSR bit 11: the link partner signalled a remote fault.
    pub remote_fault: bool,
}

/// Decode the standard link bits.
#[must_use]
pub const fn decode_bmsr(bmsr: u16, bmcr: u16) -> PhyStatus {
    PhyStatus {
        link_up: bmsr & (1 << 2) != 0,
        autoneg_complete: bmsr & (1 << 5) != 0,
        autoneg_enabled: bmcr & (1 << 12) != 0,
        remote_fault: bmsr & (1 << 11) != 0,
    }
}

/// Where the transaction is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdioState {
    /// No transaction.
    Idle,
    /// A command was sent; a response is awaited.
    Awaiting,
    /// The last transaction completed.
    Complete,
    /// A malformed response or a timeout stopped the machine.
    Faulted,
}

/// What the caller does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Send the header word (and write data, for a write).
    Transmit {
        /// The clause 22 header word after the preamble.
        header: u32,
        /// Write data, when the transaction is a write.
        write_data: Option<u16>,
    },
    /// A read completed with this value.
    Done(Option<u16>),
    /// A fault is final until `reset`.
    Fault(MdioError),
}

/// The PHY transaction state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mdio {
    state: MdioState,
    pending: Option<Transaction>,
    retries: u8,
    max_retries: u8,
}

impl Mdio {
    /// Build a machine with `max_retries` retransmissions after a timeout.
    #[must_use]
    pub const fn new(max_retries: u8) -> Self {
        Self {
            state: MdioState::Idle,
            pending: None,
            retries: 0,
            max_retries,
        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> MdioState {
        self.state
    }

    /// Retransmissions made for the current transaction.
    #[must_use]
    pub const fn retries(&self) -> u8 {
        self.retries
    }

    /// Start a transaction.
    ///
    /// # Errors
    ///
    /// Returns [`MdioError::Busy`] while another transaction is in flight,
    /// [`MdioError::Faulted`] after a fault and the address errors from [`read_command`] /
    /// [`write_command`].
    pub fn transact(&mut self, transaction: Transaction) -> Result<Action, MdioError> {
        if self.state == MdioState::Awaiting {
            return Err(MdioError::Busy);
        }
        if self.state == MdioState::Faulted {
            return Err(MdioError::Faulted);
        }
        let action = match transaction {
            Transaction::Read { phy, register } => Action::Transmit {
                header: read_command(phy, register)?,
                write_data: None,
            },
            Transaction::Write {
                phy,
                register,
                value,
            } => {
                let (header, write_data) = write_command(phy, register, value)?;
                Action::Transmit {
                    header,
                    write_data: Some(write_data),
                }
            }
        };
        self.pending = Some(transaction);
        self.retries = 0;
        self.state = MdioState::Awaiting;
        Ok(action)
    }

    /// Feed the response word.
    ///
    /// # Errors
    ///
    /// Returns [`MdioError::Idle`] when nothing is in flight and the field errors from
    /// [`decode_read`] for a malformed read response. A malformed response faults the machine.
    pub fn on_response(&mut self, word: u32) -> Result<Action, MdioError> {
        if self.state != MdioState::Awaiting {
            return Err(MdioError::Idle);
        }
        match self.pending {
            Some(Transaction::Read { .. }) => match decode_read(word) {
                Ok(value) => {
                    self.state = MdioState::Complete;
                    self.pending = None;
                    Ok(Action::Done(Some(value)))
                }
                Err(error) => {
                    self.state = MdioState::Faulted;
                    self.pending = None;
                    Err(error)
                }
            },
            Some(Transaction::Write { .. }) => {
                self.state = MdioState::Complete;
                self.pending = None;
                Ok(Action::Done(None))
            }
            None => {
                self.state = MdioState::Faulted;
                Err(MdioError::Idle)
            }
        }
    }

    /// Record a timeout; retransmit while the budget lasts, then fault.
    ///
    /// # Errors
    ///
    /// Returns [`MdioError::Idle`] when nothing is in flight and [`MdioError::Timeout`] when the
    /// retry budget is exhausted.
    pub fn on_timeout(&mut self) -> Result<Action, MdioError> {
        let Some(transaction) = self.pending else {
            return Err(MdioError::Idle);
        };
        if self.retries >= self.max_retries {
            self.state = MdioState::Faulted;
            self.pending = None;
            return Err(MdioError::Timeout);
        }
        self.retries = self.retries.saturating_add(1);
        self.state = MdioState::Awaiting;
        match transaction {
            Transaction::Read { phy, register } => Ok(Action::Transmit {
                header: read_command(phy, register)?,
                write_data: None,
            }),
            Transaction::Write {
                phy,
                register,
                value,
            } => {
                let (header, write_data) = write_command(phy, register, value)?;
                Ok(Action::Transmit {
                    header,
                    write_data: Some(write_data),
                })
            }
        }
    }

    /// Return to idle.
    pub fn reset(&mut self) {
        self.state = MdioState::Idle;
        self.pending = None;
        self.retries = 0;
    }
}

#[cfg(test)]
#[path = "mdio_ut.rs"]
mod tests;
