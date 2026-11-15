// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GENET level-2 interrupt status decoding.
//!
//! The device reports RX/TX ring completions and MDIO transactions in `INTRL2_CPU_STAT`; the
//! driver acknowledges the observed bits through `INTRL2_CPU_CLEAR`. Unknown bits are preserved
//! in the report instead of being silently dropped, so a board bring-up can see a new source.

use crate::regs::{IRQ_MDIO_DONE, IRQ_MDIO_ERROR, IRQ_RXDMA_DONE, IRQ_TXDMA_DONE};

/// Every interrupt bit this driver knows how to decode.
pub const KNOWN_INTERRUPTS: u32 = IRQ_RXDMA_DONE | IRQ_TXDMA_DONE | IRQ_MDIO_DONE | IRQ_MDIO_ERROR;

/// One `INTRL2_CPU_STAT` value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InterruptStatus(u32);

impl InterruptStatus {
    /// Wrap a raw status word.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw status word.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// RX DMA completed a batch.
    #[must_use]
    pub const fn rx_done(self) -> bool {
        self.0 & IRQ_RXDMA_DONE != 0
    }

    /// TX DMA completed a batch.
    #[must_use]
    pub const fn tx_done(self) -> bool {
        self.0 & IRQ_TXDMA_DONE != 0
    }

    /// An MDIO transaction completed.
    #[must_use]
    pub const fn mdio_done(self) -> bool {
        self.0 & IRQ_MDIO_DONE != 0
    }

    /// The MDIO block reported an error.
    #[must_use]
    pub const fn mdio_error(self) -> bool {
        self.0 & IRQ_MDIO_ERROR != 0
    }

    /// True when at least one known work source is set.
    #[must_use]
    pub const fn has_work(self) -> bool {
        self.0 & (IRQ_RXDMA_DONE | IRQ_TXDMA_DONE | IRQ_MDIO_DONE) != 0
    }

    /// Bits this driver does not decode.
    #[must_use]
    pub const fn unknown(self) -> u32 {
        self.0 & !KNOWN_INTERRUPTS
    }

    /// The value written to `INTRL2_CPU_CLEAR` to acknowledge every observed bit.
    #[must_use]
    pub const fn ack_bits(self) -> u32 {
        self.0
    }

    /// True when the status word is zero.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[cfg(test)]
#[path = "irq_ut.rs"]
mod tests;
