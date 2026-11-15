// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The xHCI control-transfer TRB sequence.
//!
//! A control transfer on endpoint 0 is a Setup Stage TRB, an optional Data Stage TRB and a Status
//! Stage TRB. The Setup Stage carries the 8 setup bytes as immediate data (the IDT bit) and the
//! transfer type (TRT): 0 no data, 2 IN, 3 OUT. The Data and Status stage direction is the DIR bit
//! (bit 16). The controller signals completion with one Transfer Event for the Status Stage, so
//! the Status Stage carries IOC.
//!
//! The field positions are the public xHCI 1.1 definition; the RP1/DWC3 integration and every
//! real-device result are UNVERIFIED.

use crate::trb::{TRB_IOC, TRB_TYPE_SHIFT, Trb, TrbType};

/// Control bit: immediate data (Setup Stage).
pub const TRB_IDT: u32 = 1 << 6;
/// Control bit: data/status direction is IN (Data/Status Stage).
pub const TRB_DIR_IN: u32 = 1 << 16;
/// Control field: setup transfer type (TRT) shift.
pub const TRB_TRT_SHIFT: u32 = 16;
/// Control field: setup transfer type mask.
pub const TRB_TRT_MASK: u32 = 0x3;

/// The direction of the data stage (or none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlDirection {
    /// No data stage.
    NoData,
    /// Device-to-host data.
    In,
    /// Host-to-device data.
    Out,
}

impl ControlDirection {
    /// The TRT field of the Setup Stage TRB.
    #[must_use]
    pub const fn trt(self) -> u32 {
        match self {
            Self::NoData => 0,
            Self::In => 2,
            Self::Out => 3,
        }
    }

    /// The direction of the Status Stage: opposite the data stage, IN when there is no data.
    #[must_use]
    pub const fn status_in(self) -> bool {
        !matches!(self, Self::In)
    }

    /// True when the data stage is device-to-host.
    #[must_use]
    pub const fn data_in(self) -> bool {
        matches!(self, Self::In)
    }
}

/// One control transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlTransfer {
    /// The 8 setup bytes.
    pub setup: [u8; 8],
    /// DMA buffer for the data stage (caller data).
    pub buffer: u64,
    /// Data-stage length in bytes (zero for a no-data transfer).
    pub length: u16,
    /// Data-stage direction.
    pub direction: ControlDirection,
}

impl ControlTransfer {
    /// Build a transfer, rejecting a data direction with a zero length or a no-data transfer with
    /// a non-zero length.
    #[must_use]
    pub const fn new(
        setup: [u8; 8],
        buffer: u64,
        length: u16,
        direction: ControlDirection,
    ) -> Option<Self> {
        match direction {
            ControlDirection::NoData if length != 0 => None,
            ControlDirection::In | ControlDirection::Out if length == 0 => None,
            _ => Some(Self {
                setup,
                buffer,
                length,
                direction,
            }),
        }
    }

    /// The number of TRBs in the sequence (2 without data, 3 with data).
    #[must_use]
    pub const fn trb_count(&self) -> u8 {
        if matches!(self.direction, ControlDirection::NoData) {
            2
        } else {
            3
        }
    }

    /// Fill `out` with the TRB sequence; returns the count (`2` or `3`).
    ///
    /// The cycle bit is left clear; the transfer ring stamps it.
    #[must_use]
    pub fn trbs(&self) -> ([Trb; 3], u8) {
        let setup = Trb::from_words([
            u32::from_le_bytes([self.setup[0], self.setup[1], self.setup[2], self.setup[3]]),
            u32::from_le_bytes([self.setup[4], self.setup[5], self.setup[6], self.setup[7]]),
            8,
            (TrbType::SetupStage.code() as u32) << TRB_TYPE_SHIFT
                | TRB_IDT
                | (self.direction.trt() << TRB_TRT_SHIFT),
        ]);
        let mut trbs = [Trb {
            parameter: 0,
            status: 0,
            control: 0,
        }; 3];
        if let Some(slot) = trbs.get_mut(0) {
            *slot = setup;
        }
        let status_in = self.direction.status_in();
        let status = Trb {
            parameter: 0,
            status: 0,
            control: (TrbType::StatusStage.code() as u32) << TRB_TYPE_SHIFT
                | (if status_in { TRB_DIR_IN } else { 0 })
                | TRB_IOC,
        };
        if matches!(self.direction, ControlDirection::NoData) {
            if let Some(slot) = trbs.get_mut(1) {
                *slot = status;
            }
            (trbs, 2)
        } else {
            let data = Trb {
                parameter: self.buffer,
                status: u32::from(self.length),
                control: (TrbType::DataStage.code() as u32) << TRB_TYPE_SHIFT
                    | (if self.direction.data_in() {
                        TRB_DIR_IN
                    } else {
                        0
                    }),
            };
            if let Some(slot) = trbs.get_mut(1) {
                *slot = data;
            }
            if let Some(slot) = trbs.get_mut(2) {
                *slot = status;
            }
            (trbs, 3)
        }
    }
}

#[cfg(test)]
#[path = "control_ut.rs"]
mod tests;
