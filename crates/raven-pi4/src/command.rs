// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SD commands and the SDHCI command/transfer-mode encoding.

use crate::response::ResponseKind;

/// Data direction of a command that carries a data transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataDirection {
    /// The card sends data to the host.
    Read,
    /// The host sends data to the card.
    Write,
}

/// SD command indices used by the EMMC2 model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdCommand {
    /// CMD0, GO_IDLE_STATE.
    Cmd0,
    /// CMD2, ALL_SEND_CID.
    Cmd2,
    /// CMD3, SEND_RELATIVE_ADDR.
    Cmd3,
    /// CMD6, SWITCH_FUNCTION.
    Cmd6,
    /// CMD7, SELECT_CARD.
    Cmd7,
    /// CMD8, SEND_IF_COND.
    Cmd8,
    /// CMD9, SEND_CSD.
    Cmd9,
    /// CMD12, STOP_TRANSMISSION.
    Cmd12,
    /// CMD16, SET_BLOCKLEN.
    Cmd16,
    /// CMD17, READ_SINGLE_BLOCK.
    Cmd17,
    /// CMD18, READ_MULTIPLE_BLOCK.
    Cmd18,
    /// CMD24, WRITE_BLOCK.
    Cmd24,
    /// CMD25, WRITE_MULTIPLE_BLOCK.
    Cmd25,
    /// CMD55, APP_CMD.
    Cmd55,
    /// ACMD6, SET_BUS_WIDTH.
    Acmd6,
    /// ACMD41, SD_SEND_OP_COND.
    Acmd41,
    /// ACMD51, SEND_SCR.
    Acmd51,
}

impl SdCommand {
    /// The six-bit command index.
    pub const fn number(self) -> u8 {
        match self {
            Self::Cmd0 => 0,
            Self::Cmd2 => 2,
            Self::Cmd3 => 3,
            Self::Cmd6 => 6,
            Self::Cmd7 => 7,
            Self::Cmd8 => 8,
            Self::Cmd9 => 9,
            Self::Cmd12 => 12,
            Self::Cmd16 => 16,
            Self::Cmd17 => 17,
            Self::Cmd18 => 18,
            Self::Cmd24 => 24,
            Self::Cmd25 => 25,
            Self::Cmd55 => 55,
            Self::Acmd6 => 6,
            Self::Acmd41 => 41,
            Self::Acmd51 => 51,
        }
    }

    /// True for an application command (issued after CMD55).
    pub const fn is_app(self) -> bool {
        matches!(self, Self::Acmd6 | Self::Acmd41 | Self::Acmd51)
    }

    /// The response kind the command expects.
    pub const fn response(self) -> ResponseKind {
        match self {
            Self::Cmd0 => ResponseKind::None,
            Self::Cmd2 | Self::Cmd9 => ResponseKind::R2,
            Self::Cmd3 => ResponseKind::R6,
            Self::Cmd7 | Self::Cmd12 => ResponseKind::R1b,
            Self::Cmd8 => ResponseKind::R7,
            Self::Acmd41 => ResponseKind::R3,
            Self::Cmd6
            | Self::Cmd16
            | Self::Cmd17
            | Self::Cmd18
            | Self::Cmd24
            | Self::Cmd25
            | Self::Cmd55
            | Self::Acmd6
            | Self::Acmd51 => ResponseKind::R1,
        }
    }

    /// The data direction, or `None` for a command without data.
    pub const fn data_direction(self) -> Option<DataDirection> {
        match self {
            Self::Cmd6 | Self::Cmd17 | Self::Cmd18 | Self::Acmd51 => Some(DataDirection::Read),
            Self::Cmd24 | Self::Cmd25 => Some(DataDirection::Write),
            _ => None,
        }
    }

    /// True for a command that transfers more than one block in one request.
    pub const fn multi_block(self) -> bool {
        matches!(self, Self::Cmd18 | Self::Cmd25)
    }

    /// A short human-readable name for logs.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cmd0 => "CMD0",
            Self::Cmd2 => "CMD2",
            Self::Cmd3 => "CMD3",
            Self::Cmd6 => "CMD6",
            Self::Cmd7 => "CMD7",
            Self::Cmd8 => "CMD8",
            Self::Cmd9 => "CMD9",
            Self::Cmd12 => "CMD12",
            Self::Cmd16 => "CMD16",
            Self::Cmd17 => "CMD17",
            Self::Cmd18 => "CMD18",
            Self::Cmd24 => "CMD24",
            Self::Cmd25 => "CMD25",
            Self::Cmd55 => "CMD55",
            Self::Acmd6 => "ACMD6",
            Self::Acmd41 => "ACMD41",
            Self::Acmd51 => "ACMD51",
        }
    }
}

/// One data transfer attached to a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataTransfer {
    /// Block size in bytes, 1..=1024 (the EMMC2 maximum block length).
    pub block_size: u16,
    /// Number of blocks, 1..=512.
    pub block_count: u16,
    /// Direction the command declares.
    pub direction: DataDirection,
    /// Ask the controller to send CMD12 automatically after a multi-block transfer.
    pub auto_cmd12: bool,
    /// Use the DMA path instead of PIO.
    pub dma: bool,
}

impl DataTransfer {
    /// A PIO transfer without automatic CMD12.
    pub fn new(
        block_size: u16,
        block_count: u16,
        direction: DataDirection,
    ) -> Result<Self, CommandError> {
        if !(1..=1024).contains(&block_size) {
            return Err(CommandError::BlockSize { value: block_size });
        }
        if !(1..=512).contains(&block_count) {
            return Err(CommandError::BlockCount { value: block_count });
        }
        Ok(Self {
            block_size,
            block_count,
            direction,
            auto_cmd12: false,
            dma: false,
        })
    }

    /// Enable automatic CMD12 after the transfer.
    pub const fn with_auto_cmd12(mut self) -> Self {
        self.auto_cmd12 = true;
        self
    }

    /// Select the DMA path.
    pub const fn with_dma(mut self) -> Self {
        self.dma = true;
        self
    }
}

/// Why a command could not be encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandError {
    /// A data transfer was supplied for a command that carries no data.
    DataNotExpected,
    /// A data-carrying command was supplied without a transfer.
    MissingData,
    /// The transfer direction does not match the command.
    DirectionMismatch {
        /// The direction the command requires.
        expected: DataDirection,
        /// The direction the transfer declares.
        got: DataDirection,
    },
    /// Block size outside 1..=1024.
    BlockSize {
        /// The rejected size.
        value: u16,
    },
    /// Block count outside 1..=512.
    BlockCount {
        /// The rejected count.
        value: u16,
    },
}

/// One SD command and its argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    command: SdCommand,
    argument: u32,
}

impl Command {
    /// A command with an argument and the response kind its index requires.
    pub const fn new(command: SdCommand, argument: u32) -> Self {
        Self { command, argument }
    }

    /// The command index.
    pub const fn command(self) -> SdCommand {
        self.command
    }

    /// The 32-bit argument.
    pub const fn argument(self) -> u32 {
        self.argument
    }

    /// The response kind the command expects.
    pub const fn response(self) -> ResponseKind {
        self.command.response()
    }

    /// Encode the command and its optional data transfer into the SDHCI registers.
    pub fn encode(&self, transfer: Option<DataTransfer>) -> Result<EncodedCommand, CommandError> {
        let expected = self.command.data_direction();
        let transfer = match (expected, transfer) {
            (None, None) => None,
            (None, Some(_)) => return Err(CommandError::DataNotExpected),
            (Some(_), None) => return Err(CommandError::MissingData),
            (Some(expected), Some(transfer)) => {
                if expected != transfer.direction {
                    return Err(CommandError::DirectionMismatch {
                        expected,
                        got: transfer.direction,
                    });
                }
                Some(transfer)
            }
        };

        let response = self.command.response();
        let mut command = u16::from(self.command.number()) << 8;
        command |= response.register_bits();
        if response.crc_check() {
            command |= COMMAND_CRC_CHECK;
        }
        if transfer.is_some() {
            command |= COMMAND_DATA_PRESENT;
        }

        let (transfer_mode, block_size, block_count) = match transfer {
            Some(transfer) => {
                let mut mode = 0u16;
                if transfer.dma {
                    mode |= TRANSFER_DMA_ENABLE;
                }
                if transfer.block_count > 1 {
                    mode |= TRANSFER_BLOCK_COUNT_ENABLE;
                }
                if transfer.auto_cmd12 {
                    mode |= TRANSFER_AUTO_CMD12_ENABLE;
                }
                if transfer.direction == DataDirection::Read {
                    mode |= TRANSFER_READ;
                }
                if self.command.multi_block() {
                    mode |= TRANSFER_MULTI_BLOCK;
                }
                (mode, transfer.block_size, transfer.block_count)
            }
            None => (0, 0, 0),
        };

        Ok(EncodedCommand {
            argument: self.argument,
            command,
            transfer_mode,
            block_size,
            block_count,
        })
    }
}

/// SDHCI command register: data-present select.
const COMMAND_DATA_PRESENT: u16 = 1 << 5;
/// SDHCI command register: CRC check enable.
const COMMAND_CRC_CHECK: u16 = 1 << 3;

/// SDHCI transfer mode: DMA enable.
const TRANSFER_DMA_ENABLE: u16 = 1 << 0;
/// SDHCI transfer mode: block count enable.
const TRANSFER_BLOCK_COUNT_ENABLE: u16 = 1 << 1;
/// SDHCI transfer mode: auto CMD12 enable.
const TRANSFER_AUTO_CMD12_ENABLE: u16 = 1 << 2;
/// SDHCI transfer mode: read direction.
const TRANSFER_READ: u16 = 1 << 4;
/// SDHCI transfer mode: multi-block transfer.
const TRANSFER_MULTI_BLOCK: u16 = 1 << 5;

/// The register values one command writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodedCommand {
    /// Command argument register.
    pub argument: u32,
    /// Command register.
    pub command: u16,
    /// Transfer mode register.
    pub transfer_mode: u16,
    /// Block size register.
    pub block_size: u16,
    /// Block count register.
    pub block_count: u16,
}

impl EncodedCommand {
    /// True when the command register declares a data transfer.
    pub const fn data_present(self) -> bool {
        self.command & COMMAND_DATA_PRESENT != 0
    }
}

/// Why a command did not complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandFailure {
    /// The controller raised a command timeout.
    Timeout,
    /// The controller raised a command CRC error.
    Crc,
    /// The controller raised a command end-bit error.
    EndBit,
    /// The controller raised a command index error.
    Index,
    /// The controller raised a data timeout.
    DataTimeout,
    /// The controller raised a data CRC error.
    DataCrc,
    /// Any other controller error, including an inhibited command line.
    Other,
}

#[cfg(test)]
#[path = "command_ut.rs"]
mod tests;
