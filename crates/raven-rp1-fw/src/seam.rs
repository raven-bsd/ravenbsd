// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The host-side seam over the RP1 boot ROM and load window.
//!
//! The kernel implementation maps these operations onto the real window: assert/deassert reset,
//! poll the boot ROM, write a chunk into the load window, ring the doorbell and read the status.
//! The offsets/encoding are UNVERIFIED, so the trait is the only hardware boundary and the host
//! tests use [`crate::sim::SimSeam`].

/// The device status after a doorbell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The chunk was accepted; the next one may be written.
    Ready,
    /// The device is still busy with the previous chunk.
    Busy,
    /// The device reported a failure code.
    Fault(u16),
}

/// Why a seam operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeamError {
    /// A bus/register access failed.
    Bus,
    /// The operation did not complete in the poll budget.
    Timeout,
}

/// The boot-ROM/load-window operations the loader drives.
pub trait FirmwareSeam {
    /// Assert and release the device reset.
    fn reset(&mut self) -> Result<(), SeamError>;

    /// True when the boot ROM is ready for the image.
    fn boot_ready(&mut self) -> Result<bool, SeamError>;

    /// Write `data` at `offset` in the load window.
    fn write_chunk(&mut self, offset: u32, data: &[u8]) -> Result<(), SeamError>;

    /// Ring the doorbell for the chunk at `offset`.
    fn doorbell(&mut self, offset: u32) -> Result<(), SeamError>;

    /// Read the device status.
    fn status(&mut self) -> Result<Status, SeamError>;

    /// Start the loaded image.
    fn start(&mut self) -> Result<(), SeamError>;
}
