// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The SD card initialisation state machine.
//!
//! [`Init`] owns the sequence, not the registers: `start` returns the first command, the caller
//! executes it and feeds [`Init::advance`] the four response words (or a controller failure). The
//! machine then returns the next command or the ready card, so the host driver never carries the
//! sequence in its own control flow and a wrong or failed command has one typed error path.
//!
//! The sequence is the one the B42 spike extracted: CMD0, CMD8 (with a timeout fallback to a v1
//! card), CMD55/ACMD41, CMD2, CMD3, CMD9, CMD7, CMD55/ACMD6 and CMD16. Data commands are not part
//! of this state machine.

use crate::command::{Command, CommandFailure, SdCommand};
use crate::response::{CardStatus, Cid, Csd, Response, ResponseKind, decode};
use crate::timing::BusWidth;

/// CMD8 argument: 2.7-3.6 V and the 0xAA check pattern.
pub const CMD8_ARGUMENT: u32 = 0x1AA;
/// ACMD41 argument for an SDHC/SDXC card (HCS and the 3.2-3.4 V window).
pub const ACMD41_HCS_ARGUMENT: u32 = 0x40FF_8000;
/// ACMD41 argument for a v1 card (no HCS).
pub const ACMD41_LEGACY_ARGUMENT: u32 = 0x00FF_8000;
/// Largest number of ACMD41 "busy" rounds before the card is declared unresponsive.
pub const MAX_OP_COND_ATTEMPTS: u8 = 100;
/// Block length programmed by CMD16.
pub const BLOCK_LEN: u32 = 512;

/// The state reached after the last accepted command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitPhase {
    /// Before CMD0 completes.
    Idle,
    /// CMD8 (SEND_IF_COND).
    IfCond,
    /// CMD55/ACMD41 voltage and capacity negotiation.
    OpCond,
    /// CMD2 (ALL_SEND_CID).
    Cid,
    /// CMD3 (SEND_RELATIVE_ADDR).
    Rca,
    /// CMD9 (SEND_CSD).
    Csd,
    /// CMD7 (SELECT_CARD).
    Select,
    /// CMD55/ACMD6 bus-width negotiation.
    BusWidth,
    /// CMD16 (SET_BLOCKLEN).
    BlockLength,
    /// The card is selected, 4-bit capable and has a 512-byte block length.
    Ready,
}

/// A card that completed initialisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    /// Relative card address assigned by CMD3.
    pub rca: u16,
    /// True for SDHC/SDXC (CCS set and not a v1 fallback).
    pub high_capacity: bool,
    /// Capacity in bytes from the CSD.
    pub capacity_bytes: u64,
    /// Card identification.
    pub cid: Cid,
    /// Card-specific data.
    pub csd: Csd,
    /// True when CMD8 timed out and the v1 path was used.
    pub legacy: bool,
}

/// The next action the caller must take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitStep {
    /// Execute this command and call [`Init::advance`] with its outcome.
    Send(Command),
    /// Initialisation finished.
    Ready(Card),
}

/// Why initialisation stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// The controller reported a failure for the command.
    CommandFailed {
        /// The command that failed.
        command: SdCommand,
        /// The controller failure.
        failure: CommandFailure,
    },
    /// The response kind did not match the command.
    UnexpectedResponse {
        /// The command whose response was decoded.
        command: SdCommand,
        /// The response kind the command requires.
        expected: ResponseKind,
    },
    /// The card status reports an error.
    CardError {
        /// The command whose response carried the error.
        command: SdCommand,
        /// The card status.
        status: CardStatus,
    },
    /// The card did not echo the 2.7-3.6 V window.
    UnsupportedVoltage {
        /// The accepted-voltage field from R7.
        accepted: u8,
    },
    /// The card did not echo the 0xAA check pattern.
    EchoMismatch {
        /// The pattern from R7.
        pattern: u8,
    },
    /// The response to CMD55 did not set APP_CMD.
    AppCommandNotAccepted {
        /// The command whose response lacked APP_CMD.
        command: SdCommand,
    },
    /// CMD3 returned relative card address zero, which the specification reserves.
    NoRelativeAddress,
    /// ACMD41 stayed busy for [`MAX_OP_COND_ATTEMPTS`] rounds.
    Acmd41Timeout {
        /// The number of rounds attempted.
        attempts: u8,
    },
    /// The CSD capacity is zero or its fields overflow.
    InvalidCapacity,
    /// The CID or CSD was never captured.
    MissingIdentity,
    /// The state machine was advanced after it reported a ready card.
    OutOfOrder,
}

/// The card-initialisation state machine.
#[derive(Clone, Debug)]
pub struct Init {
    phase: InitPhase,
    expected: SdCommand,
    attempts: u8,
    legacy: bool,
    high_capacity: bool,
    rca: u16,
    cid: Option<Cid>,
    csd: Option<Csd>,
}

impl Init {
    /// A machine waiting for CMD0.
    pub const fn new() -> Self {
        Self {
            phase: InitPhase::Idle,
            expected: SdCommand::Cmd0,
            attempts: 0,
            legacy: false,
            high_capacity: false,
            rca: 0,
            cid: None,
            csd: None,
        }
    }

    /// The current phase.
    pub const fn phase(&self) -> InitPhase {
        self.phase
    }

    /// Start the sequence by returning CMD0.
    pub const fn start(&self) -> InitStep {
        InitStep::Send(Command::new(SdCommand::Cmd0, 0))
    }

    /// The number of ACMD41 rounds already spent waiting for the card.
    pub const fn op_cond_attempts(&self) -> u8 {
        self.attempts
    }

    /// True when the v1 fallback path was taken.
    pub const fn legacy(&self) -> bool {
        self.legacy
    }

    /// Feed one command outcome and receive the next step.
    pub fn advance(
        &mut self,
        outcome: Result<[u32; 4], CommandFailure>,
    ) -> Result<InitStep, InitError> {
        if self.phase == InitPhase::Ready {
            return Err(InitError::OutOfOrder);
        }
        let command = self.expected;

        let words = match outcome {
            Ok(words) => words,
            Err(CommandFailure::Timeout) if command == SdCommand::Cmd8 && !self.legacy => {
                // A v1 card does not answer CMD8. The specification's fallback is the
                // non-HCS ACMD41 path; any other timeout is a real failure.
                self.legacy = true;
                return Ok(self.send(SdCommand::Cmd55, 0, InitPhase::OpCond));
            }
            Err(failure) => return Err(InitError::CommandFailed { command, failure }),
        };

        let response = decode(command.response(), words);
        match command {
            SdCommand::Cmd0 => Ok(self.send(SdCommand::Cmd8, CMD8_ARGUMENT, InitPhase::IfCond)),
            SdCommand::Cmd8 => {
                let echo = match response {
                    Response::R7(echo) => echo,
                    _ => return Err(self.unexpected(command)),
                };
                if echo.voltage_accepted() != 1 {
                    return Err(InitError::UnsupportedVoltage {
                        accepted: echo.voltage_accepted(),
                    });
                }
                if echo.check_pattern() != 0xAA {
                    return Err(InitError::EchoMismatch {
                        pattern: echo.check_pattern(),
                    });
                }
                Ok(self.send(SdCommand::Cmd55, 0, InitPhase::OpCond))
            }
            SdCommand::Cmd55 => {
                let status = match response {
                    Response::R1(status) => status,
                    _ => return Err(self.unexpected(command)),
                };
                require_card_ok(status, command)?;
                if !status.app_cmd() {
                    return Err(InitError::AppCommandNotAccepted { command });
                }
                if self.phase == InitPhase::BusWidth {
                    let argument = BusWidth::Bit4.acmd6_argument();
                    Ok(self.send(SdCommand::Acmd6, argument, InitPhase::BusWidth))
                } else {
                    let argument = if self.legacy {
                        ACMD41_LEGACY_ARGUMENT
                    } else {
                        ACMD41_HCS_ARGUMENT
                    };
                    Ok(self.send(SdCommand::Acmd41, argument, InitPhase::OpCond))
                }
            }
            SdCommand::Acmd41 => {
                let ocr = match response {
                    Response::R3(ocr) => ocr,
                    _ => return Err(self.unexpected(command)),
                };
                if !ocr.ready() {
                    self.attempts = self.attempts.saturating_add(1);
                    if self.attempts >= MAX_OP_COND_ATTEMPTS {
                        return Err(InitError::Acmd41Timeout {
                            attempts: self.attempts,
                        });
                    }
                    return Ok(self.send(SdCommand::Cmd55, 0, InitPhase::OpCond));
                }
                self.high_capacity = !self.legacy && ocr.capacity_status();
                Ok(self.send(SdCommand::Cmd2, 0, InitPhase::Cid))
            }
            SdCommand::Cmd2 => {
                let raw = match response {
                    Response::R2(raw) => raw,
                    _ => return Err(self.unexpected(command)),
                };
                self.cid = Some(Cid::from_raw(raw));
                Ok(self.send(SdCommand::Cmd3, 0, InitPhase::Rca))
            }
            SdCommand::Cmd3 => {
                let (rca, status) = match response {
                    Response::R6 { rca, status } => (rca, status),
                    _ => return Err(self.unexpected(command)),
                };
                require_card_ok(status, command)?;
                if rca == 0 {
                    return Err(InitError::NoRelativeAddress);
                }
                self.rca = rca;
                Ok(self.send(SdCommand::Cmd9, u32::from(rca), InitPhase::Csd))
            }
            SdCommand::Cmd9 => {
                let raw = match response {
                    Response::R2(raw) => raw,
                    _ => return Err(self.unexpected(command)),
                };
                let csd = Csd::from_raw(raw);
                match csd.capacity_bytes() {
                    Some(capacity) if capacity > 0 => self.csd = Some(csd),
                    _ => return Err(InitError::InvalidCapacity),
                }
                Ok(self.send(SdCommand::Cmd7, u32::from(self.rca), InitPhase::Select))
            }
            SdCommand::Cmd7 => {
                let status = match response {
                    Response::R1(status) => status,
                    _ => return Err(self.unexpected(command)),
                };
                require_card_ok(status, command)?;
                Ok(self.send(SdCommand::Cmd55, u32::from(self.rca), InitPhase::BusWidth))
            }
            SdCommand::Acmd6 => {
                let status = match response {
                    Response::R1(status) => status,
                    _ => return Err(self.unexpected(command)),
                };
                require_card_ok(status, command)?;
                Ok(self.send(SdCommand::Cmd16, BLOCK_LEN, InitPhase::BlockLength))
            }
            SdCommand::Cmd16 => {
                let status = match response {
                    Response::R1(status) => status,
                    _ => return Err(self.unexpected(command)),
                };
                require_card_ok(status, command)?;
                let cid = self.cid.ok_or(InitError::MissingIdentity)?;
                let csd = self.csd.ok_or(InitError::MissingIdentity)?;
                let capacity_bytes = csd.capacity_bytes().ok_or(InitError::InvalidCapacity)?;
                self.phase = InitPhase::Ready;
                Ok(InitStep::Ready(Card {
                    rca: self.rca,
                    high_capacity: self.high_capacity,
                    capacity_bytes,
                    cid,
                    csd,
                    legacy: self.legacy,
                }))
            }
            _ => Err(InitError::OutOfOrder),
        }
    }

    fn send(&mut self, command: SdCommand, argument: u32, phase: InitPhase) -> InitStep {
        self.expected = command;
        self.phase = phase;
        InitStep::Send(Command::new(command, argument))
    }

    fn unexpected(&self, command: SdCommand) -> InitError {
        InitError::UnexpectedResponse {
            command,
            expected: command.response(),
        }
    }
}

impl Default for Init {
    fn default() -> Self {
        Self::new()
    }
}

fn require_card_ok(status: CardStatus, command: SdCommand) -> Result<(), InitError> {
    if status.command_error() {
        Err(InitError::CardError { command, status })
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "init_ut.rs"]
mod tests;
