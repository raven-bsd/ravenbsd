// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable GENET frame path.
//!
//! [`FrameBackend`] joins the typed bring-up controller to the RX/TX ownership model. It takes
//! packet buffers from the [`Seam`], posts and completes descriptors, applies debounced link
//! events to `UMAC_CMD`/`EXT_RGMII_OOB_CTRL`, and leaves the real MMIO/DMA behind the seam.

use crate::{Buffer, Controller, DriverError, Seam, Stage};
use raven_genet::frame::{FramePath, RxCompletion, RxPost, TxCompletion, TxPost};
use raven_genet::link::{LinkEvent, LinkState, LinkTracker};
use raven_genet::phy;
use raven_genet::regs;
use raven_genet::ring::{BUFFER_LEN, RxStatus, TxStatus};

/// A bring-up controller plus the live frame rings and link tracker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameBackend<S> {
    controller: Controller<S>,
    path: FramePath,
    link: LinkTracker,
}

impl<S: Seam> FrameBackend<S> {
    /// Create the frame path after the controller reached [`Stage::Ready`].
    pub fn new(controller: Controller<S>) -> Result<Self, DriverError> {
        Self::with_debounce(controller, 2)
    }

    /// Create the frame path with an explicit link debounce.
    pub fn with_debounce(controller: Controller<S>, debounce: u8) -> Result<Self, DriverError> {
        if controller.stage() != Stage::Ready {
            return Err(DriverError::WrongStage {
                expected: Stage::Ready,
                actual: controller.stage(),
            });
        }
        Ok(Self {
            controller,
            path: FramePath::new(regs::DESC_COUNT)?,
            link: LinkTracker::new(debounce),
        })
    }

    /// The bring-up controller.
    #[must_use]
    pub const fn controller(&self) -> &Controller<S> {
        &self.controller
    }

    /// The frame ownership path.
    #[must_use]
    pub const fn path(&self) -> &FramePath {
        &self.path
    }

    /// The stable link state.
    #[must_use]
    pub const fn link_state(&self) -> LinkState {
        self.link.state()
    }

    /// Post the next RX packet buffer to the device.
    pub fn post_rx_next(&mut self) -> Result<RxPost, DriverError> {
        let index = self.path.rx.indices().producer_slot();
        let buffer = self.rx_buffer(index)?;
        Ok(self.path.rx.post_next(buffer.bus_addr)?)
    }

    /// Refill the next RX packet buffer; the operation is the same as [`Self::post_rx_next`].
    pub fn refill_rx_next(&mut self) -> Result<RxPost, DriverError> {
        self.post_rx_next()
    }

    /// Complete the next RX descriptor with the status read from the device.
    pub fn complete_rx(&mut self, status: RxStatus) -> Result<Option<RxCompletion>, DriverError> {
        Ok(self.path.rx.complete_next(status)?)
    }

    /// Queue the next TX frame in the next free packet buffer.
    pub fn queue_tx_next(&mut self, length: usize) -> Result<TxPost, DriverError> {
        let index = self.path.tx.indices().producer_slot();
        let buffer = self.tx_buffer(index)?;
        if buffer.len < length {
            return Err(DriverError::ShortBuffer {
                index,
                len: buffer.len,
            });
        }
        Ok(self.path.tx.queue_next(buffer.bus_addr, length)?)
    }

    /// Reclaim the next TX descriptor with the status read from the device.
    pub fn reclaim_tx(&mut self, status: TxStatus) -> Result<Option<TxCompletion>, DriverError> {
        Ok(self.path.tx.reclaim_next(status)?)
    }

    /// Feed one PHY sample and apply a debounced link transition.
    pub fn observe_link(
        &mut self,
        bmsr: u16,
        auxsts: u16,
    ) -> Result<Option<LinkEvent>, DriverError> {
        let event = self.link.observe(bmsr, auxsts)?;
        if let Some(event) = event {
            self.apply_link(event)?;
        }
        Ok(event)
    }

    fn apply_link(&mut self, event: LinkEvent) -> Result<(), DriverError> {
        let link_up = !matches!(event, LinkEvent::Down);
        let current = self.controller.seam.read32(regs::EXT_RGMII_OOB_CTRL)?;
        let value = phy::ext_rgmii_oob_ctrl(current, self.controller.info().phy_mode, link_up);
        self.controller
            .seam
            .write32(regs::EXT_RGMII_OOB_CTRL, value)?;
        if let LinkEvent::Up { speed, .. } | LinkEvent::Changed { speed, .. } = event {
            let command = self.controller.seam.read32(regs::UMAC_CMD)?;
            let command = (command & !regs::UMAC_CMD_SPEED_MASK)
                | (speed.umac_bits() << regs::UMAC_CMD_SPEED_SHIFT);
            self.controller.seam.write32(regs::UMAC_CMD, command)?;
        }
        Ok(())
    }

    fn rx_buffer(&mut self, index: usize) -> Result<Buffer, DriverError> {
        let buffer = self
            .controller
            .seam
            .rx_buffer(index)
            .ok_or(DriverError::MissingBuffer(index))?;
        if buffer.len < BUFFER_LEN {
            return Err(DriverError::ShortBuffer {
                index,
                len: buffer.len,
            });
        }
        Ok(buffer)
    }

    fn tx_buffer(&mut self, index: usize) -> Result<Buffer, DriverError> {
        self.controller
            .seam
            .tx_buffer(index)
            .ok_or(DriverError::MissingBuffer(index))
    }
}

#[cfg(test)]
#[path = "frame_ut.rs"]
mod tests;
