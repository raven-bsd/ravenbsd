// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable GENET frame path.
//!
//! [`FrameBackend`] joins the typed bring-up controller to the RX/TX ownership model. It takes
//! packet buffers from the [`Seam`], posts and completes descriptors, applies debounced link
//! events to `UMAC_CMD`/`EXT_RGMII_OOB_CTRL`, and leaves the real MMIO/DMA behind the seam.

use crate::{Buffer, Controller, DriverError, Seam, Stage};
use raven_genet::filter::FilterTable;
use raven_genet::frame::{
    DEFAULT_RECLAIM_BUDGET, FramePath, ReclaimBudget, ReclaimReport, RxCompletion, RxPost,
    TxCompletion, TxPost,
};
use raven_genet::irq::InterruptStatus;
use raven_genet::link::{LinkEvent, LinkState, LinkTracker};
use raven_genet::mac::MacAddress;
use raven_genet::phy;
use raven_genet::recovery::{ErrorRecovery, RecoveryAction};
use raven_genet::regs;
use raven_genet::ring::{BUFFER_LEN, RxStatus, TxStatus};

/// The result of one interrupt service pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceReport {
    /// Decoded interrupt status.
    pub status: InterruptStatus,
    /// The bounded RX/TX reclaim result.
    pub reclaim: ReclaimReport,
    /// An MDIO transaction completed.
    pub mdio_done: bool,
    /// The MDIO block reported an error.
    pub mdio_error: bool,
    /// Interrupt bits the driver does not decode.
    pub unknown: u32,
    /// Value written to `INTRL2_CPU_CLEAR`.
    pub ack: u32,
    /// Recovery action after this pass.
    pub recovery: RecoveryAction,
}

/// A bring-up controller plus the live frame rings, link tracker and error policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameBackend<S> {
    controller: Controller<S>,
    path: FramePath,
    link: LinkTracker,
    recovery: ErrorRecovery,
}

impl<S: Seam> FrameBackend<S> {
    /// Create the frame path after the controller reached [`Stage::Ready`].
    pub fn new(controller: Controller<S>) -> Result<Self, DriverError> {
        Self::with_debounce(controller, 2)
    }

    /// Create the frame path with an explicit link debounce.
    pub fn with_debounce(controller: Controller<S>, debounce: u8) -> Result<Self, DriverError> {
        Self::with_counts(controller, regs::DESC_COUNT, regs::DESC_COUNT, debounce)
    }

    /// Create the frame path with independent RX/TX slot counts.
    ///
    /// The B39 `nic:` service maps eight RX buffers and three TX buffers from the shared pool,
    /// so the driver uses those counts while the hardware descriptor array remains the Pi 4
    /// default.
    pub fn with_counts(
        controller: Controller<S>,
        rx_count: u16,
        tx_count: u16,
        debounce: u8,
    ) -> Result<Self, DriverError> {
        if controller.stage() != Stage::Ready {
            return Err(DriverError::WrongStage {
                expected: Stage::Ready,
                actual: controller.stage(),
            });
        }
        Ok(Self {
            controller,
            path: FramePath::with_counts(rx_count, tx_count)?,
            link: LinkTracker::new(debounce),
            recovery: ErrorRecovery::default(),
        })
    }

    /// The bring-up controller.
    #[must_use]
    pub const fn controller(&self) -> &Controller<S> {
        &self.controller
    }

    /// Mutable access to the bring-up controller (target bring-up and host tests).
    pub fn controller_mut(&mut self) -> &mut Controller<S> {
        &mut self.controller
    }

    /// The platform seam.
    #[must_use]
    pub const fn seam(&self) -> &S {
        &self.controller.seam
    }

    /// Mutable access to the platform seam.
    pub fn seam_mut(&mut self) -> &mut S {
        &mut self.controller.seam
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

    /// The bounded error-recovery policy.
    #[must_use]
    pub const fn recovery(&self) -> ErrorRecovery {
        self.recovery
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

    /// Queue a TX frame at an explicit device-visible address.
    ///
    /// The B39 `nic:` pool already holds the frame, so the driver points the GENET descriptor at
    /// that buffer instead of copying it into a seam slot.
    pub fn queue_tx_address(&mut self, address: u64, length: usize) -> Result<TxPost, DriverError> {
        Ok(self.path.tx.queue_next(address, length)?)
    }

    /// Reclaim the next TX descriptor with the status read from the device.
    pub fn reclaim_tx(&mut self, status: TxStatus) -> Result<Option<TxCompletion>, DriverError> {
        Ok(self.path.tx.reclaim_next(status)?)
    }

    /// Write one RX descriptor to the hardware descriptor array.
    pub fn write_rx_descriptor(&mut self, post: RxPost) -> Result<(), DriverError> {
        let registers = post.descriptor.registers();
        let status = regs::rx_desc_status(post.index).ok_or(DriverError::BadInfo)?;
        self.controller.seam.write32(status, registers[0])?;
        self.controller.seam.write32(status + 0x04, registers[1])?;
        self.controller.seam.write32(status + 0x08, registers[2])?;
        Ok(())
    }

    /// Write one TX descriptor to the hardware descriptor array.
    pub fn write_tx_descriptor(&mut self, post: TxPost) -> Result<(), DriverError> {
        let registers = post.descriptor.registers();
        let status = regs::tx_desc_status(post.index).ok_or(DriverError::BadInfo)?;
        self.controller.seam.write32(status, registers[0])?;
        self.controller.seam.write32(status + 0x04, registers[1])?;
        self.controller.seam.write32(status + 0x08, registers[2])?;
        Ok(())
    }

    /// Publish the RX producer/consumer counters to the hardware queue registers.
    pub fn publish_rx_indices(&mut self) -> Result<(), DriverError> {
        let indices = self.path.rx.indices();
        let queue = regs::DEFAULT_QUEUE;
        let producer = regs::rx_dma_prod_index(queue).ok_or(DriverError::BadInfo)?;
        let consumer = regs::rx_dma_cons_index(queue).ok_or(DriverError::BadInfo)?;
        self.controller
            .seam
            .write32(producer, u32::from(indices.hardware_producer()))?;
        self.controller
            .seam
            .write32(consumer, u32::from(indices.hardware_consumer()))?;
        Ok(())
    }

    /// Publish the TX producer/consumer counters to the hardware queue registers.
    pub fn publish_tx_indices(&mut self) -> Result<(), DriverError> {
        let indices = self.path.tx.indices();
        let queue = regs::DEFAULT_QUEUE;
        let producer = regs::tx_dma_prod_index(queue).ok_or(DriverError::BadInfo)?;
        let consumer = regs::tx_dma_cons_index(queue).ok_or(DriverError::BadInfo)?;
        self.controller
            .seam
            .write32(producer, u32::from(indices.hardware_producer()))?;
        self.controller
            .seam
            .write32(consumer, u32::from(indices.hardware_consumer()))?;
        Ok(())
    }

    /// Read the next consumer RX status word from the hardware descriptor array.
    pub fn rx_consumer_status(&mut self) -> Result<RxStatus, DriverError> {
        let slot = self.path.rx.indices().consumer_slot() as u16;
        let offset = regs::rx_desc_status(slot).ok_or(DriverError::BadInfo)?;
        Ok(RxStatus::from_raw(self.controller.seam.read32(offset)?))
    }

    /// Read the next consumer TX status word from the hardware descriptor array.
    pub fn tx_consumer_status(&mut self) -> Result<TxStatus, DriverError> {
        let slot = self.path.tx.indices().consumer_slot() as u16;
        let offset = regs::tx_desc_status(slot).ok_or(DriverError::BadInfo)?;
        Ok(TxStatus::from_raw(self.controller.seam.read32(offset)?))
    }

    /// Read `INTRL2_CPU_STAT`.
    pub fn interrupt_status(&mut self) -> Result<u32, DriverError> {
        Ok(self.controller.seam.interrupt_status()?)
    }

    /// Acknowledge interrupt bits through `INTRL2_CPU_CLEAR`.
    pub fn ack_interrupt(&mut self, bits: u32) -> Result<(), DriverError> {
        self.controller.seam.write32(regs::INTRL2_CPU_CLEAR, bits)?;
        Ok(())
    }

    /// Read the PHY `BMSR` and Broadcom `AUXSTS` registers.
    pub fn sample_link(&mut self) -> Result<(u16, u16), DriverError> {
        let phy = self.controller.info().phy_address;
        let bmsr = crate::board::read_mdio(&mut self.controller.seam, phy, phy::MII_BMSR)?;
        let auxsts =
            crate::board::read_mdio(&mut self.controller.seam, phy, phy::BRGPHY_MII_AUXSTS)?;
        Ok((bmsr, auxsts))
    }

    /// Sample the PHY and apply a debounced link transition.
    pub fn poll_link(&mut self) -> Result<Option<LinkEvent>, DriverError> {
        let (bmsr, auxsts) = self.sample_link()?;
        self.observe_link(bmsr, auxsts)
    }

    /// Program the Pi 4 RX/TX ring registers and enable the DMA engines.
    ///
    /// The descriptor array is the hardware-internal 256-entry array; the B39 service uses the
    /// first eight RX and three TX entries. The exact ring configuration is UNVERIFIED on a
    /// board; the host tests check the documented register writes.
    pub fn program_rings(&mut self) -> Result<(), DriverError> {
        let setup = Controller::<S>::ring_setup();
        let queue = regs::DEFAULT_QUEUE;
        let reg = |offset: Option<usize>| offset.ok_or(DriverError::BadInfo);

        self.controller
            .seam
            .write32(reg(regs::rx_dma_ring_buf_size(queue))?, setup.buf_size)?;
        self.controller
            .seam
            .write32(reg(regs::rx_dma_start_addr_lo(queue))?, 0)?;
        self.controller
            .seam
            .write32(reg(regs::rx_dma_start_addr_hi(queue))?, 0)?;
        self.controller
            .seam
            .write32(reg(regs::rx_dma_end_addr_lo(queue))?, setup.end_address)?;
        self.controller
            .seam
            .write32(reg(regs::rx_dma_end_addr_hi(queue))?, 0)?;
        self.controller
            .seam
            .write32(reg(regs::rx_dma_prod_index(queue))?, 0)?;
        self.controller
            .seam
            .write32(reg(regs::rx_dma_cons_index(queue))?, 0)?;
        self.controller
            .seam
            .write32(reg(regs::rx_dma_xon_xoff_thres(queue))?, setup.xon_xoff)?;
        self.controller.seam.write32(
            reg(regs::rx_dma_mbuf_done_thres(queue))?,
            setup.rx_threshold,
        )?;
        self.controller.seam.write32(
            reg(regs::rx_dma_ring_timeout(queue))?,
            setup.rx_timeout_ticks,
        )?;
        self.controller.seam.write32(regs::RX_DMA_RING_CFG, 0)?;
        self.controller.seam.write32(
            regs::RX_DMA_CTRL,
            regs::RX_DMA_CTRL_EN | regs::rx_dma_ctrl_rbuf_en(queue),
        )?;

        self.controller
            .seam
            .write32(reg(regs::tx_dma_ring_buf_size(queue))?, setup.buf_size)?;
        self.controller
            .seam
            .write32(reg(regs::tx_dma_start_addr_lo(queue))?, 0)?;
        self.controller
            .seam
            .write32(reg(regs::tx_dma_start_addr_hi(queue))?, 0)?;
        self.controller
            .seam
            .write32(reg(regs::tx_dma_end_addr_lo(queue))?, setup.end_address)?;
        self.controller
            .seam
            .write32(reg(regs::tx_dma_end_addr_hi(queue))?, 0)?;
        self.controller
            .seam
            .write32(reg(regs::tx_dma_prod_index(queue))?, 0)?;
        self.controller
            .seam
            .write32(reg(regs::tx_dma_cons_index(queue))?, 0)?;
        self.controller.seam.write32(
            reg(regs::tx_dma_mbuf_done_thres(queue))?,
            setup.tx_threshold,
        )?;
        self.controller.seam.write32(regs::TX_DMA_RING_CFG, 0)?;
        self.controller.seam.write32(
            regs::TX_DMA_CTRL,
            regs::TX_DMA_CTRL_EN | regs::tx_dma_ctrl_rbuf_en(queue),
        )?;

        self.controller
            .seam
            .write32(regs::RBUF_CTRL, regs::RBUF_ALIGN_2B)?;
        self.controller
            .seam
            .write32(regs::UMAC_MAX_FRAME_LEN, 1518)?;
        let command = self.controller.seam.read32(regs::UMAC_CMD)?;
        self.controller.seam.write32(
            regs::UMAC_CMD,
            command | regs::UMAC_CMD_RXEN | regs::UMAC_CMD_TXEN,
        )?;
        Ok(())
    }

    /// Read and service one interrupt-status pass.
    pub fn service_irq(&mut self, budget: ReclaimBudget) -> Result<ServiceReport, DriverError> {
        let raw = self.controller.seam.interrupt_status()?;
        self.service_interrupt(raw, budget)
    }

    /// Service one raw interrupt-status word.
    ///
    /// The RX/TX descriptor statuses are read for the consumer slots, the frame path reclaims at
    /// most the budget, and the observed bits are acknowledged through `INTRL2_CPU_CLEAR`.
    pub fn service_interrupt(
        &mut self,
        raw: u32,
        budget: ReclaimBudget,
    ) -> Result<ServiceReport, DriverError> {
        let status = InterruptStatus::from_raw(raw);
        let budget = ReclaimBudget::new(
            budget.max_rx.min(DEFAULT_RECLAIM_BUDGET),
            budget.max_tx.min(DEFAULT_RECLAIM_BUDGET),
        );
        let mut rx_statuses = [RxStatus::from_raw(0); DEFAULT_RECLAIM_BUDGET];
        let mut tx_statuses = [TxStatus::from_raw(0); DEFAULT_RECLAIM_BUDGET];
        let mut rx_count = 0usize;
        let mut tx_count = 0usize;

        if status.rx_done() {
            while rx_count < budget.max_rx && self.path.rx.posted()? > 0 {
                let slot = self.path.rx.indices().consumer_slot() as u16;
                let offset = regs::rx_desc_status(slot).ok_or(DriverError::BadInfo)?;
                rx_statuses[rx_count] = RxStatus::from_raw(self.controller.seam.read32(offset)?);
                rx_count += 1;
            }
        }
        if status.tx_done() {
            while tx_count < budget.max_tx && self.path.tx.posted()? > 0 {
                let slot = self.path.tx.indices().consumer_slot() as u16;
                let offset = regs::tx_desc_status(slot).ok_or(DriverError::BadInfo)?;
                tx_statuses[tx_count] = TxStatus::from_raw(self.controller.seam.read32(offset)?);
                tx_count += 1;
            }
        }

        let reclaim =
            self.path
                .reclaim(budget, &rx_statuses[..rx_count], &tx_statuses[..tx_count])?;
        let ack = status.ack_bits();
        if ack != 0 {
            self.controller.seam.write32(regs::INTRL2_CPU_CLEAR, ack)?;
        }
        let recovery = if reclaim.rx_errors > 0 || status.mdio_error() {
            self.recovery.record_error()
        } else {
            self.recovery.record_success();
            RecoveryAction::None
        };
        Ok(ServiceReport {
            status,
            reclaim,
            mdio_done: status.mdio_done(),
            mdio_error: status.mdio_error(),
            unknown: status.unknown(),
            ack,
            recovery,
        })
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

    /// Alias for [`Self::observe_link`] used by the IRQ/link loop.
    pub fn service_link(
        &mut self,
        bmsr: u16,
        auxsts: u16,
    ) -> Result<Option<LinkEvent>, DriverError> {
        self.observe_link(bmsr, auxsts)
    }

    /// Program the broadcast, unicast and multicast filter table.
    pub fn program_filters(
        &mut self,
        own: MacAddress,
        multicast: &[MacAddress],
        promiscuous: bool,
    ) -> Result<(), DriverError> {
        let mut table = FilterTable::new(own);
        table.set_multicast(multicast)?;
        table.set_promiscuous(promiscuous);
        for slot in 0..table.slot_count() {
            let Some(write) = table.write(slot) else {
                continue;
            };
            let addr0 = regs::umac_mdf_addr0(slot).ok_or(DriverError::BadInfo)?;
            let addr1 = regs::umac_mdf_addr1(slot).ok_or(DriverError::BadInfo)?;
            self.controller.seam.write32(addr0, write.addr0)?;
            self.controller.seam.write32(addr1, write.addr1)?;
        }
        self.controller
            .seam
            .write32(regs::UMAC_MDF_CTRL, table.mdf_ctrl())?;
        let command = self.controller.seam.read32(regs::UMAC_CMD)?;
        let command = (command & !regs::UMAC_CMD_PROMISC) | table.command_bits();
        self.controller.seam.write32(regs::UMAC_CMD, command)?;
        Ok(())
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
