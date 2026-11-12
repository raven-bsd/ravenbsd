// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Block backend abstraction and the deterministic RAM disk used before a real
//! storage driver exists.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_proto_block::{BlockInfo, SECTOR_SIZE};

/// A sector-addressed block device.
pub trait Device {
    /// Geometry and read-only state.
    fn info(&self) -> BlockInfo;

    /// Read whole sectors starting at `sector`.
    fn read_sectors(&mut self, sector: u64, buffer: &mut [u8]) -> Result<(), Errno>;

    /// Write whole sectors starting at `sector`.
    fn write_sectors(&mut self, sector: u64, data: &[u8]) -> Result<(), Errno>;

    /// Flush volatile state.  A RAM disk has none, but the operation is still
    /// accounted for.
    fn flush(&mut self) -> Result<(), Errno>;
}

/// An in-memory read-write disk.
pub struct RamDisk {
    data: Vec<u8>,
    read_only: bool,
}

impl RamDisk {
    /// Create a disk over `data`, which must be a nonempty whole number of
    /// sectors.
    pub fn new(data: Vec<u8>, read_only: bool) -> Result<Self, Errno> {
        if data.is_empty() || !data.len().is_multiple_of(SECTOR_SIZE as usize) {
            return Err(Errno::EINVAL);
        }
        Ok(Self { data, read_only })
    }

    /// Number of whole sectors.
    pub fn sectors(&self) -> u64 {
        (self.data.len() / SECTOR_SIZE as usize) as u64
    }

    fn range(&self, sector: u64, length: usize) -> Result<core::ops::Range<usize>, Errno> {
        if length == 0 || !length.is_multiple_of(SECTOR_SIZE as usize) {
            return Err(Errno::EINVAL);
        }
        let count = length / SECTOR_SIZE as usize;
        let count = u64::try_from(count).map_err(|_| Errno::EINVAL)?;
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

impl Device for RamDisk {
    fn info(&self) -> BlockInfo {
        BlockInfo {
            sector_size: SECTOR_SIZE,
            read_only: u32::from(self.read_only),
            sectors: self.sectors(),
            reserved: 0,
        }
    }

    fn read_sectors(&mut self, sector: u64, buffer: &mut [u8]) -> Result<(), Errno> {
        let range = self.range(sector, buffer.len())?;
        buffer.copy_from_slice(&self.data[range]);
        Ok(())
    }

    fn write_sectors(&mut self, sector: u64, data: &[u8]) -> Result<(), Errno> {
        if self.read_only {
            return Err(Errno::EROFS);
        }
        let range = self.range(sector, data.len())?;
        self.data[range].copy_from_slice(data);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "device_ut.rs"]
mod tests;
