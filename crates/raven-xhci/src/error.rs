// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! xHCI controller errors.

use core::fmt;

/// Why a controller operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XhciError {
    /// An MMIO or DMA access was refused.
    Access,
    /// A controller/event poll exceeded its budget.
    Timeout,
    /// The command or transfer ring has no free slot.
    RingFull,
    /// The capability registers are not usable.
    Capability,
    /// The slot is zero or outside the controller.
    Slot(u8),
    /// A command/transfer completed with a non-success code.
    Completion(u8),
    /// The operation is not supported by this controller.
    Unsupported,
    /// The requested port is not connected.
    NoPort,
    /// The context/descriptor is too long or too short.
    Length,
}

impl fmt::Display for XhciError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Access => f.write_str("xHCI MMIO/DMA access failed"),
            Self::Timeout => f.write_str("xHCI operation timed out"),
            Self::RingFull => f.write_str("xHCI ring is full"),
            Self::Capability => f.write_str("xHCI capability registers are unusable"),
            Self::Slot(slot) => write!(f, "xHCI slot {slot} is invalid"),
            Self::Completion(code) => write!(f, "xHCI completion code {code}"),
            Self::Unsupported => f.write_str("xHCI operation is unsupported"),
            Self::NoPort => f.write_str("xHCI port is not connected"),
            Self::Length => f.write_str("xHCI context length is invalid"),
        }
    }
}

impl core::error::Error for XhciError {}
