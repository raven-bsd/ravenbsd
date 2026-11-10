// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Block device abstraction.

use crate::error::IoError;

/// A random-access block device supplied by the host.
///
/// Reads and writes are byte-addressed. The filesystem never calls [`BlockIo::write_at`] or
/// [`BlockIo::flush`] while mounted read-only, but they are part of the contract so the same
/// device can be handed to the later write-capable tickets.
pub trait BlockIo {
    /// Size of the device in bytes.
    fn size_bytes(&self) -> u64;

    /// Read exactly `buf.len()` bytes at `offset`.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError>;

    /// Write exactly `buf.len()` bytes at `offset`.
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError>;

    /// Flush any cached writes to the device.
    fn flush(&mut self) -> Result<(), IoError>;
}
