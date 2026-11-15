// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `block:` request mapping for the EMMC2 driver and a host-testable backend.
//!
//! `blockd` opens the driver's `disk:` scheme and speaks `raven-proto-block`: `INFO`, `READ`,
//! `WRITE` and `FLUSH`. This module decodes that protocol, maps every data request to a
//! [`raven_pi4::transfer::MultiBlockTransfer`] (which selects CMD17/18/24/25 and validates the
//! block geometry), and hands the mapped plan to a [`BlockDevice`] backend. The backend is the
//! seam: [`MemoryDisk`] is the host-testable implementation; the real card backend uses the DMA
//! and PIO paths behind the same trait.
//!
//! `LIST` is deliberately rejected here. The partition list belongs to `blockd`, which scans the
//! GPT itself; the driver only serves the disk geometry and sectors.

use core::ops::Range;

use raven_abi::errno::Errno;
use raven_pi4::command::{Command, DataDirection, DataTransfer};
use raven_pi4::transfer::{MultiBlockTransfer, TransferError, TransferErrorKind};
use raven_proto_block::{
    BlockInfo, INFO_LEN, MAX_SECTORS, Op, ProtoError, REPLY_HEADER_LEN, Request, SECTOR_SIZE,
    encode_reply, write_reply_header,
};

/// A sector-addressed backend behind the driver.
pub trait BlockDevice {
    /// Geometry and read-only state.
    fn info(&self) -> BlockInfo;

    /// Read the sectors named by `plan` into `buffer`.
    fn read(&mut self, plan: &BlockPlan, buffer: &mut [u8]) -> Result<(), Errno>;

    /// Write `data` to the sectors named by `plan`.
    fn write(&mut self, plan: &BlockPlan, data: &[u8]) -> Result<(), Errno>;

    /// Flush volatile state.
    fn flush(&mut self) -> Result<(), Errno>;
}

/// A protocol request mapped to one card transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockPlan {
    transfer: MultiBlockTransfer,
    command: Command,
    transfer_flags: DataTransfer,
}

impl BlockPlan {
    /// Validate a sector range and build the command and data-transfer values.
    pub fn new(
        direction: DataDirection,
        sector: u64,
        count: u32,
        capacity: u64,
    ) -> Result<Self, BlockError> {
        if count == 0 || count > MAX_SECTORS {
            return Err(BlockError::OutOfRange);
        }
        let end = sector
            .checked_add(u64::from(count))
            .ok_or(BlockError::OutOfRange)?;
        if end > capacity {
            return Err(BlockError::OutOfRange);
        }
        let blocks = u16::try_from(count).map_err(|_| BlockError::OutOfRange)?;
        let mut transfer = MultiBlockTransfer::new(direction, sector, SECTOR_SIZE as u16, blocks)
            .map_err(BlockError::Transfer)?;
        let start = transfer.start().map_err(BlockError::Transfer)?;
        Ok(Self {
            transfer,
            command: start.command,
            transfer_flags: start.transfer,
        })
    }

    /// The command the card backend must issue.
    pub const fn command(self) -> Command {
        self.command
    }

    /// The data-transfer values for the command encoding.
    pub const fn transfer_flags(self) -> DataTransfer {
        self.transfer_flags
    }

    /// First sector.
    pub const fn sector(self) -> u64 {
        self.transfer.start_lba()
    }

    /// Number of sectors.
    pub const fn blocks(self) -> u16 {
        self.transfer.total_blocks()
    }

    /// Direction.
    pub const fn direction(self) -> DataDirection {
        self.transfer.direction()
    }

    /// Transfer length in bytes.
    pub const fn bytes(self) -> usize {
        self.transfer.total_bytes() as usize
    }
}

/// Why the block service could not process a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockError {
    /// The caller supplied a response buffer smaller than the reply header.
    ReplyTooSmall,
    /// A protocol reply could not be encoded.
    Encode(ProtoError),
    /// The backend geometry is not a valid 512-byte sector device.
    BadGeometry,
    /// The sector range is empty, overflows or lies outside the device.
    OutOfRange,
    /// The request could not be mapped to a card transfer.
    Transfer(TransferError),
}

impl BlockError {
    /// The errno to encode in an error reply.
    pub const fn errno(self) -> Errno {
        match self {
            Self::ReplyTooSmall | Self::BadGeometry | Self::OutOfRange => Errno::EINVAL,
            Self::Encode(_) => Errno::EPROTO,
            Self::Transfer(error) => match error.kind {
                TransferErrorKind::ZeroBlocks
                | TransferErrorKind::TooManyBlocks
                | TransferErrorKind::BadBlockSize
                | TransferErrorKind::AddressTooHigh => Errno::EINVAL,
                _ => Errno::EIO,
            },
        }
    }
}

/// A `block:` service over one [`BlockDevice`].
pub struct BlockService<D: BlockDevice> {
    device: D,
}

impl<D: BlockDevice> BlockService<D> {
    /// Wrap a backend after validating its geometry.
    pub fn new(device: D) -> Result<Self, BlockError> {
        let info = device.info();
        if info.sector_size != SECTOR_SIZE || info.sectors == 0 {
            return Err(BlockError::BadGeometry);
        }
        Ok(Self { device })
    }

    /// The backend geometry.
    pub fn info(&self) -> BlockInfo {
        self.device.info()
    }

    /// Handle one protocol request and write the reply into `response`.
    ///
    /// A malformed or out-of-range request produces a valid error reply; the return value is the
    /// number of reply bytes. `Err` is reserved for a caller-side buffer too small to hold even the
    /// reply header, or a reply-encoding failure.
    pub fn handle(&mut self, request: &[u8], response: &mut [u8]) -> Result<usize, BlockError> {
        let decoded = match Request::decode(request) {
            Ok(decoded) => decoded,
            Err(_) => return self.reply_error(response, Errno::EINVAL),
        };
        let info = self.device.info();
        if info.sector_size != SECTOR_SIZE || info.sectors == 0 {
            return self.reply_error(response, Errno::EINVAL);
        }
        match decoded.op {
            Op::Read => {
                let plan = match BlockPlan::new(
                    DataDirection::Read,
                    decoded.sector,
                    decoded.count,
                    info.sectors,
                ) {
                    Ok(plan) => plan,
                    Err(error) => return self.reply_error(response, error.errno()),
                };
                let bytes = plan.bytes();
                let total = REPLY_HEADER_LEN
                    .checked_add(bytes)
                    .ok_or(BlockError::ReplyTooSmall)?;
                if response.len() < total {
                    return self.reply_error(response, Errno::EINVAL);
                }
                if let Err(error) = self
                    .device
                    .read(&plan, &mut response[REPLY_HEADER_LEN..total])
                {
                    return self.reply_error(response, error);
                }
                self.write_header(response, 0, bytes)?;
                Ok(total)
            }
            Op::Write => {
                if info.read_only != 0 {
                    return self.reply_error(response, Errno::EROFS);
                }
                let plan = match BlockPlan::new(
                    DataDirection::Write,
                    decoded.sector,
                    decoded.count,
                    info.sectors,
                ) {
                    Ok(plan) => plan,
                    Err(error) => return self.reply_error(response, error.errno()),
                };
                if plan.bytes() != decoded.data.len() {
                    return self.reply_error(response, Errno::EINVAL);
                }
                if let Err(error) = self.device.write(&plan, decoded.data) {
                    return self.reply_error(response, error);
                }
                self.reply_ok(response)
            }
            Op::Flush => match self.device.flush() {
                Ok(()) => self.reply_ok(response),
                Err(error) => self.reply_error(response, error),
            },
            Op::Info => {
                let mut payload = [0u8; INFO_LEN];
                info.encode(&mut payload).map_err(BlockError::Encode)?;
                self.reply_payload(response, &payload)
            }
            Op::List => self.reply_error(response, Errno::EOPNOTSUPP),
        }
    }

    fn reply_ok(&self, response: &mut [u8]) -> Result<usize, BlockError> {
        self.reply_payload(response, &[])
    }

    fn reply_error(&self, response: &mut [u8], error: Errno) -> Result<usize, BlockError> {
        match encode_reply(response, -i32::from(error.0), &[]) {
            Ok(size) => Ok(size),
            Err(ProtoError::TooShort) => Err(BlockError::ReplyTooSmall),
            Err(other) => Err(BlockError::Encode(other)),
        }
    }

    fn reply_payload(&self, response: &mut [u8], payload: &[u8]) -> Result<usize, BlockError> {
        match encode_reply(response, 0, payload) {
            Ok(size) => Ok(size),
            Err(ProtoError::TooShort) => Err(BlockError::ReplyTooSmall),
            Err(other) => Err(BlockError::Encode(other)),
        }
    }

    fn write_header(
        &self,
        response: &mut [u8],
        status: i32,
        payload_len: usize,
    ) -> Result<(), BlockError> {
        match write_reply_header(response, status, payload_len) {
            Ok(()) => Ok(()),
            Err(ProtoError::TooShort) => Err(BlockError::ReplyTooSmall),
            Err(other) => Err(BlockError::Encode(other)),
        }
    }
}

/// A host-testable sector backend over a borrowed byte slice.
pub struct MemoryDisk<'a> {
    data: &'a mut [u8],
    read_only: bool,
}

impl<'a> MemoryDisk<'a> {
    /// Wrap a whole-sector buffer.
    pub fn new(data: &'a mut [u8], read_only: bool) -> Result<Self, Errno> {
        if data.is_empty() || !data.len().is_multiple_of(SECTOR_SIZE as usize) {
            return Err(Errno::EINVAL);
        }
        Ok(Self { data, read_only })
    }

    /// Number of whole sectors.
    pub fn sectors(&self) -> u64 {
        (self.data.len() / SECTOR_SIZE as usize) as u64
    }

    fn range(&self, sector: u64, length: usize) -> Result<Range<usize>, Errno> {
        if length == 0 || !length.is_multiple_of(SECTOR_SIZE as usize) {
            return Err(Errno::EINVAL);
        }
        let count = u64::try_from(length / SECTOR_SIZE as usize).map_err(|_| Errno::EINVAL)?;
        let end = sector.checked_add(count).ok_or(Errno::EINVAL)?;
        if end > self.sectors() {
            return Err(Errno::EINVAL);
        }
        let start = usize::try_from(sector)
            .map_err(|_| Errno::EINVAL)?
            .checked_mul(SECTOR_SIZE as usize)
            .ok_or(Errno::EINVAL)?;
        let end = start.checked_add(length).ok_or(Errno::EINVAL)?;
        Ok(start..end)
    }
}

impl BlockDevice for MemoryDisk<'_> {
    fn info(&self) -> BlockInfo {
        BlockInfo {
            sector_size: SECTOR_SIZE,
            read_only: u32::from(self.read_only),
            sectors: self.sectors(),
            reserved: 0,
        }
    }

    fn read(&mut self, plan: &BlockPlan, buffer: &mut [u8]) -> Result<(), Errno> {
        if plan.direction() != DataDirection::Read || buffer.len() != plan.bytes() {
            return Err(Errno::EINVAL);
        }
        let range = self.range(plan.sector(), buffer.len())?;
        buffer.copy_from_slice(&self.data[range]);
        Ok(())
    }

    fn write(&mut self, plan: &BlockPlan, data: &[u8]) -> Result<(), Errno> {
        if self.read_only {
            return Err(Errno::EROFS);
        }
        if plan.direction() != DataDirection::Write || data.len() != plan.bytes() {
            return Err(Errno::EINVAL);
        }
        let range = self.range(plan.sector(), data.len())?;
        self.data[range].copy_from_slice(data);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "block_ut.rs"]
mod tests;
