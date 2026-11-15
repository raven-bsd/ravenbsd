// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Multi-block data-transfer state and error paths.
//!
//! The driver owns the registers and DMA setup; this module owns the sequence of one CMD17/18/24/25
//! transfer. [`MultiBlockTransfer::start`] returns the command and the data-transfer values the
//! caller encodes. The caller then feeds command completion, optional PIO block progress and the
//! transfer-complete or error interrupt. Every transition is typed, so a duplicate or out-of-order
//! event fails instead of silently completing a request.

use crate::command::{Command, CommandFailure, DataDirection, DataTransfer, SdCommand};
use crate::registers::{
    ERROR_ADMA, ERROR_AUTO_CMD12, ERROR_CMD_CRC, ERROR_CMD_END_BIT, ERROR_CMD_INDEX,
    ERROR_CMD_TIMEOUT, ERROR_DATA_CRC, ERROR_DATA_TIMEOUT, ERROR_DMA, ErrorInterrupt,
    NormalInterrupt,
};

/// Largest block count one SDHCI command can carry.
pub const MAX_BLOCKS_PER_TRANSFER: u16 = 512;

/// State of one multi-block transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferState {
    /// The command has not been issued.
    Idle,
    /// The command is in flight.
    Command,
    /// The data phase is active.
    Data,
    /// Every block completed.
    Complete,
    /// A controller error or a protocol violation stopped the transfer.
    Failed,
}

/// Why a transfer stopped or could not start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferErrorKind {
    /// The request had zero blocks.
    ZeroBlocks,
    /// The request exceeded [`MAX_BLOCKS_PER_TRANSFER`].
    TooManyBlocks,
    /// The block size was zero or above the ADMA2/EMMC2 maximum.
    BadBlockSize,
    /// The start address does not fit the 32-bit command argument.
    AddressTooHigh,
    /// A block-progress event reported zero blocks.
    InvalidProgress,
    /// Progress or completion disagrees with the requested block count.
    BlockCountMismatch,
    /// The command timed out.
    CommandTimeout,
    /// The command response failed CRC.
    CommandCrc,
    /// The command response had a bad end bit.
    CommandEndBit,
    /// The command index did not match.
    CommandIndex,
    /// The data phase timed out.
    DataTimeout,
    /// The data phase failed CRC.
    DataCrc,
    /// The ADMA engine failed.
    Adma,
    /// The system DMA engine failed.
    Dma,
    /// Automatic CMD12 failed.
    AutoCmd12,
    /// Any other controller error.
    Other,
    /// An event arrived in a state that does not accept it.
    UnexpectedEvent,
    /// The transfer already completed.
    AlreadyComplete,
}

impl TransferErrorKind {
    /// Map a command failure from the register layer.
    pub const fn from_command_failure(failure: CommandFailure) -> Self {
        match failure {
            CommandFailure::Timeout => Self::CommandTimeout,
            CommandFailure::Crc => Self::CommandCrc,
            CommandFailure::EndBit => Self::CommandEndBit,
            CommandFailure::Index => Self::CommandIndex,
            CommandFailure::DataTimeout => Self::DataTimeout,
            CommandFailure::DataCrc => Self::DataCrc,
            CommandFailure::Other => Self::Other,
        }
    }

    /// Map the error interrupt bits, command errors taking priority over data errors.
    pub const fn from_error_interrupt(error: ErrorInterrupt) -> Self {
        let raw = error.raw();
        if raw & ERROR_CMD_TIMEOUT != 0 {
            Self::CommandTimeout
        } else if raw & ERROR_CMD_CRC != 0 {
            Self::CommandCrc
        } else if raw & ERROR_CMD_END_BIT != 0 {
            Self::CommandEndBit
        } else if raw & ERROR_CMD_INDEX != 0 {
            Self::CommandIndex
        } else if raw & ERROR_DATA_TIMEOUT != 0 {
            Self::DataTimeout
        } else if raw & ERROR_DATA_CRC != 0 {
            Self::DataCrc
        } else if raw & ERROR_ADMA != 0 {
            Self::Adma
        } else if raw & ERROR_DMA != 0 {
            Self::Dma
        } else if raw & ERROR_AUTO_CMD12 != 0 {
            Self::AutoCmd12
        } else {
            Self::Other
        }
    }
}

/// A transfer error together with the progress at the failure point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferError {
    /// The failure kind.
    pub kind: TransferErrorKind,
    /// Blocks reported complete before the failure.
    pub blocks_done: u16,
    /// Blocks still outstanding at the failure.
    pub blocks_remaining: u16,
}

impl TransferError {
    /// Build an error snapshot.
    pub const fn new(kind: TransferErrorKind, blocks_done: u16, blocks_remaining: u16) -> Self {
        Self {
            kind,
            blocks_done,
            blocks_remaining,
        }
    }
}

/// The command and transfer values for the next step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferStart {
    /// The command to encode and issue.
    pub command: Command,
    /// The data-transfer values for the command encoding.
    pub transfer: DataTransfer,
}

/// What one status poll produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferEvent {
    /// No state-changing bit was present.
    Idle,
    /// The command completed and the data phase is now active.
    CommandAccepted,
    /// The data phase is active and no completion bit is present yet.
    Data,
    /// Every block completed.
    Complete {
        /// Blocks completed.
        blocks: u16,
        /// Bytes transferred.
        bytes: u32,
    },
}

/// One CMD17/18/24/25 transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MultiBlockTransfer {
    direction: DataDirection,
    start_lba: u64,
    block_size: u16,
    total_blocks: u16,
    blocks_done: u16,
    state: TransferState,
    failure: Option<TransferErrorKind>,
}

impl MultiBlockTransfer {
    /// Validate and create a transfer.
    pub fn new(
        direction: DataDirection,
        start_lba: u64,
        block_size: u16,
        total_blocks: u16,
    ) -> Result<Self, TransferError> {
        if total_blocks == 0 {
            return Err(TransferError::new(TransferErrorKind::ZeroBlocks, 0, 0));
        }
        if total_blocks > MAX_BLOCKS_PER_TRANSFER {
            return Err(TransferError::new(
                TransferErrorKind::TooManyBlocks,
                0,
                total_blocks,
            ));
        }
        if block_size == 0 || block_size > 1024 {
            return Err(TransferError::new(
                TransferErrorKind::BadBlockSize,
                0,
                total_blocks,
            ));
        }
        let last_lba = start_lba
            .checked_add(u64::from(total_blocks) - 1)
            .ok_or_else(|| {
                TransferError::new(TransferErrorKind::AddressTooHigh, 0, total_blocks)
            })?;
        if last_lba > u64::from(u32::MAX) {
            return Err(TransferError::new(
                TransferErrorKind::AddressTooHigh,
                0,
                total_blocks,
            ));
        }
        Ok(Self {
            direction,
            start_lba,
            block_size,
            total_blocks,
            blocks_done: 0,
            state: TransferState::Idle,
            failure: None,
        })
    }

    /// The transfer direction.
    pub const fn direction(self) -> DataDirection {
        self.direction
    }

    /// The first block address.
    pub const fn start_lba(self) -> u64 {
        self.start_lba
    }

    /// Block size in bytes.
    pub const fn block_size(self) -> u16 {
        self.block_size
    }

    /// Total number of blocks.
    pub const fn total_blocks(self) -> u16 {
        self.total_blocks
    }

    /// Blocks reported complete.
    pub const fn blocks_done(self) -> u16 {
        self.blocks_done
    }

    /// Blocks still outstanding.
    pub const fn remaining_blocks(self) -> u16 {
        self.total_blocks - self.blocks_done
    }

    /// Bytes reported complete.
    pub const fn bytes_done(self) -> u32 {
        self.blocks_done as u32 * self.block_size as u32
    }

    /// Total transfer length in bytes.
    pub const fn total_bytes(self) -> u32 {
        self.total_blocks as u32 * self.block_size as u32
    }

    /// Current state.
    pub const fn state(self) -> TransferState {
        self.state
    }

    /// The failure that stopped the transfer, if any.
    pub const fn failure(self) -> Option<TransferErrorKind> {
        self.failure
    }

    /// Issue the command for this transfer.
    pub fn start(&mut self) -> Result<TransferStart, TransferError> {
        if let Some(error) = self.existing_error() {
            return Err(error);
        }
        if self.state != TransferState::Idle {
            return Err(self.fail(TransferErrorKind::UnexpectedEvent));
        }
        let command = match (self.direction, self.total_blocks) {
            (DataDirection::Read, 1) => SdCommand::Cmd17,
            (DataDirection::Read, _) => SdCommand::Cmd18,
            (DataDirection::Write, 1) => SdCommand::Cmd24,
            (DataDirection::Write, _) => SdCommand::Cmd25,
        };
        let transfer = DataTransfer::new(self.block_size, self.total_blocks, self.direction)
            .map_err(|_| self.fail(TransferErrorKind::BadBlockSize))?;
        self.state = TransferState::Command;
        Ok(TransferStart {
            command: Command::new(command, self.start_lba as u32),
            transfer,
        })
    }

    /// The command completed and the data phase started.
    pub fn command_complete(&mut self) -> Result<(), TransferError> {
        if let Some(error) = self.existing_error() {
            return Err(error);
        }
        if self.state != TransferState::Command {
            return Err(self.fail(TransferErrorKind::UnexpectedEvent));
        }
        self.state = TransferState::Data;
        Ok(())
    }

    /// Report PIO block progress.
    pub fn block_complete(&mut self, blocks: u16) -> Result<(), TransferError> {
        if let Some(error) = self.existing_error() {
            return Err(error);
        }
        if self.state != TransferState::Data {
            return Err(self.fail(TransferErrorKind::UnexpectedEvent));
        }
        if blocks == 0 {
            return Err(self.fail(TransferErrorKind::InvalidProgress));
        }
        let Some(done) = self.blocks_done.checked_add(blocks) else {
            return Err(self.fail(TransferErrorKind::BlockCountMismatch));
        };
        if done > self.total_blocks {
            return Err(self.fail(TransferErrorKind::BlockCountMismatch));
        }
        self.blocks_done = done;
        Ok(())
    }

    /// The controller reported transfer complete; accept any blocks not reported individually.
    pub fn transfer_complete(&mut self) -> Result<u32, TransferError> {
        if let Some(error) = self.existing_error() {
            return Err(error);
        }
        match self.state {
            TransferState::Data => {
                self.blocks_done = self.total_blocks;
                self.state = TransferState::Complete;
                Ok(self.total_bytes())
            }
            TransferState::Complete => Err(self.error(TransferErrorKind::AlreadyComplete)),
            _ => Err(self.fail(TransferErrorKind::UnexpectedEvent)),
        }
    }

    /// Poll one SDHCI interrupt pair.
    pub fn poll(
        &mut self,
        normal: NormalInterrupt,
        error: ErrorInterrupt,
    ) -> Result<TransferEvent, TransferError> {
        if self.state == TransferState::Failed {
            return Err(self.error(self.failure.unwrap_or(TransferErrorKind::Other)));
        }
        if error.any() {
            return Err(self.fail(TransferErrorKind::from_error_interrupt(error)));
        }
        if normal.error_interrupt() {
            return Err(self.fail(TransferErrorKind::Other));
        }
        if self.state == TransferState::Idle {
            return if normal.command_complete() || normal.transfer_complete() {
                Err(self.fail(TransferErrorKind::UnexpectedEvent))
            } else {
                Ok(TransferEvent::Idle)
            };
        }
        let mut accepted = false;
        if normal.command_complete() {
            if self.state != TransferState::Command {
                return Err(self.fail(TransferErrorKind::UnexpectedEvent));
            }
            self.state = TransferState::Data;
            accepted = true;
        }
        if normal.transfer_complete() {
            if self.state != TransferState::Data {
                return Err(self.fail(TransferErrorKind::UnexpectedEvent));
            }
            self.blocks_done = self.total_blocks;
            self.state = TransferState::Complete;
            return Ok(TransferEvent::Complete {
                blocks: self.total_blocks,
                bytes: self.total_bytes(),
            });
        }
        if accepted {
            Ok(TransferEvent::CommandAccepted)
        } else {
            match self.state {
                TransferState::Data => Ok(TransferEvent::Data),
                TransferState::Complete => Err(self.error(TransferErrorKind::AlreadyComplete)),
                _ => Ok(TransferEvent::Idle),
            }
        }
    }

    /// Mark the transfer failed and snapshot the error.
    pub fn fail(&mut self, kind: TransferErrorKind) -> TransferError {
        self.state = TransferState::Failed;
        self.failure = Some(kind);
        self.error(kind)
    }

    fn error(self, kind: TransferErrorKind) -> TransferError {
        TransferError::new(kind, self.blocks_done, self.remaining_blocks())
    }

    fn existing_error(self) -> Option<TransferError> {
        self.failure.map(|kind| self.error(kind))
    }
}

#[cfg(test)]
#[path = "transfer_ut.rs"]
mod tests;
