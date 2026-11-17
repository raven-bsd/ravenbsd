// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GENET userspace driver skeleton.
//!
//! The pure register, ring, MDIO, PHY and device-tree rules live in [`raven_genet`]. This crate
//! is the seam between that model and the real device: [`Seam`] is the only place that will own
//! MMIO and DMA, so the host tests drive a fake register file while the board wiring remains an
//! integration step.
//!
//! The bring-up is a typed sequence: `Attached -> Probed -> MacProgrammed -> RgmiiProgrammed ->
//! Ready`. A step called out of order returns [`DriverError::WrongStage`] instead of touching the
//! device in an undefined state.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(test)]
extern crate std;

use raven_genet::dtb::GenetInfo;
use raven_genet::filter::FilterError;
use raven_genet::frame::FrameError;
use raven_genet::mac::MacAddress;
use raven_genet::mdio::MdioError;
use raven_genet::phy;
use raven_genet::phy::LinkError;
use raven_genet::regs;
use raven_genet::release::ReleaseError;
use raven_genet::ring::{
    RingError, RingIndices, RingSetup, RxDescriptor, RxStatus, TxDescriptor, TxStatus,
};

pub mod board;
pub mod frame;
pub mod nic;
pub mod options;
pub mod platform;
pub mod release;

/// A read or write refused by the seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeamError {
    /// The offset is outside the mapped window.
    Bounds,
    /// The access is not naturally aligned.
    Alignment,
    /// The underlying device access failed.
    Io,
}

/// One DMA buffer visible to the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Buffer {
    /// Device-visible bus address.
    pub bus_addr: u64,
    /// Buffer length in bytes.
    pub len: usize,
}

/// The MMIO/DMA operations the real driver must implement.
///
/// The host tests use a fake register file; the board implementation will wrap
/// `raven_driver::MmioRegion` and `raven_driver::DmaRegion`.
pub trait Seam {
    /// Read one 32-bit register.
    fn read32(&mut self, offset: usize) -> Result<u32, SeamError>;
    /// Write one 32-bit register.
    fn write32(&mut self, offset: usize, value: u32) -> Result<(), SeamError>;
    /// Return the RX packet buffer for `index`.
    fn rx_buffer(&mut self, index: usize) -> Option<Buffer>;
    /// Return the TX packet buffer for `index`.
    fn tx_buffer(&mut self, index: usize) -> Option<Buffer>;

    /// Read `INTRL2_CPU_STAT` for the interrupt loop.
    fn interrupt_status(&mut self) -> Result<u32, SeamError> {
        self.read32(regs::INTRL2_CPU_STAT)
    }

    /// Wait between MDIO busy polls. The host tests use the no-op default.
    fn delay_us(&mut self, _microseconds: u32) {}
}

/// The typed bring-up stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The controller has a configuration and no device access yet.
    Attached,
    /// The GENET revision has been read and accepted.
    Probed,
    /// `UMAC_MAC0`/`UMAC_MAC1` have been written.
    MacProgrammed,
    /// `SYS_PORT_CTRL` and `EXT_RGMII_OOB_CTRL` have been written.
    RgmiiProgrammed,
    /// The configuration is complete; the frame pump can start.
    Ready,
}

/// Why a bring-up step failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverError {
    /// The parsed device-tree data is not usable.
    BadInfo,
    /// The revision register is not a GENET v5 block.
    WrongRevision(u32),
    /// The address is zero, broadcast or multicast.
    InvalidMac,
    /// A step was called out of order.
    WrongStage {
        /// The stage the step requires.
        expected: Stage,
        /// The stage the controller is in.
        actual: Stage,
    },
    /// The seam refused an access.
    Seam(SeamError),
    /// The requested packet buffer is not mapped.
    MissingBuffer(usize),
    /// The packet buffer is shorter than the request.
    ShortBuffer {
        /// Buffer index.
        index: usize,
        /// Buffer length.
        len: usize,
    },
    /// The ring model rejected an index operation.
    Ring(RingError),
    /// The frame ownership path rejected an operation.
    Frame(FrameError),
    /// The PHY link tracker rejected a sample.
    Link(LinkError),
    /// The filter table rejected an address list.
    Filter(FilterError),
    /// The MDIO command could not be built.
    Mdio(MdioError),
    /// The MDIO transaction did not finish within its poll budget.
    MdioTimeout,
    /// The release network gate rejected an observation.
    Release(ReleaseError),
}

impl From<SeamError> for DriverError {
    fn from(error: SeamError) -> Self {
        Self::Seam(error)
    }
}

impl From<RingError> for DriverError {
    fn from(error: RingError) -> Self {
        Self::Ring(error)
    }
}

impl From<FrameError> for DriverError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

impl From<LinkError> for DriverError {
    fn from(error: LinkError) -> Self {
        Self::Link(error)
    }
}

impl From<FilterError> for DriverError {
    fn from(error: FilterError) -> Self {
        Self::Filter(error)
    }
}

impl From<MdioError> for DriverError {
    fn from(error: MdioError) -> Self {
        Self::Mdio(error)
    }
}

impl From<ReleaseError> for DriverError {
    fn from(error: ReleaseError) -> Self {
        Self::Release(error)
    }
}

/// The host-testable GENET controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Controller<S> {
    seam: S,
    info: GenetInfo,
    stage: Stage,
    rx: RingIndices,
    tx: RingIndices,
}

impl<S: Seam> Controller<S> {
    /// Attach a seam and a parsed device-tree description.
    pub fn attach(seam: S, info: GenetInfo) -> Result<Self, DriverError> {
        if info.size < regs::WINDOW_SIZE as u64
            || u64::from(info.mdio_offset) + u64::from(info.mdio_size) > info.size
            || info.phy_address > raven_genet::mdio::MAX_PHY_ADDRESS
        {
            return Err(DriverError::BadInfo);
        }
        Ok(Self {
            seam,
            info,
            stage: Stage::Attached,
            rx: RingIndices::new(regs::DESC_COUNT)?,
            tx: RingIndices::new(regs::DESC_COUNT)?,
        })
    }

    /// The device-tree data.
    #[must_use]
    pub const fn info(&self) -> &GenetInfo {
        &self.info
    }

    /// The current bring-up stage.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    /// The RX ring indices.
    #[must_use]
    pub const fn rx(&self) -> RingIndices {
        self.rx
    }

    /// The TX ring indices.
    #[must_use]
    pub const fn tx(&self) -> RingIndices {
        self.tx
    }

    /// The ring values the driver programs during initialisation.
    #[must_use]
    pub const fn ring_setup() -> RingSetup {
        RingSetup::pi4_default()
    }

    /// Read and validate the GENET revision.
    pub fn probe(&mut self) -> Result<u32, DriverError> {
        self.require(Stage::Attached)?;
        let revision = self.seam.read32(regs::SYS_REV_CTRL)?;
        if !regs::revision_is_v5(revision) {
            return Err(DriverError::WrongRevision(revision));
        }
        self.stage = Stage::Probed;
        Ok(revision)
    }

    /// Program `UMAC_MAC0`/`UMAC_MAC1`.
    pub fn program_mac(&mut self, address: MacAddress) -> Result<(), DriverError> {
        self.require(Stage::Probed)?;
        if !address.is_valid() {
            return Err(DriverError::InvalidMac);
        }
        let (mac0, mac1) = address.umac_words();
        self.seam.write32(regs::UMAC_MAC0, mac0)?;
        self.seam.write32(regs::UMAC_MAC1, u32::from(mac1))?;
        self.stage = Stage::MacProgrammed;
        Ok(())
    }

    /// Program the external-GPHY port mode and the RGMII delay control.
    pub fn program_rgmii(&mut self, link_up: bool) -> Result<u32, DriverError> {
        self.require(Stage::MacProgrammed)?;
        self.seam
            .write32(regs::SYS_PORT_CTRL, regs::SYS_PORT_MODE_EXT_GPHY)?;
        let current = self.seam.read32(regs::EXT_RGMII_OOB_CTRL)?;
        let value = phy::ext_rgmii_oob_ctrl(current, self.info.phy_mode, link_up);
        self.seam.write32(regs::EXT_RGMII_OOB_CTRL, value)?;
        self.stage = Stage::RgmiiProgrammed;
        Ok(value)
    }

    /// Mark the controller ready after the ring registers have been programmed.
    pub fn ready(&mut self) -> Result<(), DriverError> {
        self.require(Stage::RgmiiProgrammed)?;
        self.stage = Stage::Ready;
        Ok(())
    }

    /// Build the RX descriptor for one packet buffer.
    pub fn rx_descriptor(
        &mut self,
        index: u16,
        status: RxStatus,
    ) -> Result<RxDescriptor, DriverError> {
        let buffer = self
            .seam
            .rx_buffer(index as usize)
            .ok_or(DriverError::MissingBuffer(index as usize))?;
        Ok(RxDescriptor::new(buffer.bus_addr, status))
    }

    /// Build the TX descriptor for one packet buffer.
    pub fn tx_descriptor(
        &mut self,
        index: u16,
        status: TxStatus,
    ) -> Result<TxDescriptor, DriverError> {
        let buffer = self
            .seam
            .tx_buffer(index as usize)
            .ok_or(DriverError::MissingBuffer(index as usize))?;
        Ok(TxDescriptor::new(buffer.bus_addr, status))
    }

    /// Advance the producer index after handing `count` descriptors to the device.
    pub fn publish_rx(&mut self, count: usize) -> Result<(), DriverError> {
        self.rx.advance_producer(count)?;
        Ok(())
    }

    /// Advance the consumer index after retiring `count` completed RX descriptors.
    pub fn retire_rx(&mut self, count: usize) -> Result<(), DriverError> {
        self.rx.advance_consumer(count)?;
        Ok(())
    }

    /// Advance the producer index after queueing `count` TX descriptors.
    pub fn publish_tx(&mut self, count: usize) -> Result<(), DriverError> {
        self.tx.advance_producer(count)?;
        Ok(())
    }

    /// Advance the consumer index after retiring `count` completed TX descriptors.
    pub fn retire_tx(&mut self, count: usize) -> Result<(), DriverError> {
        self.tx.advance_consumer(count)?;
        Ok(())
    }

    fn require(&self, expected: Stage) -> Result<(), DriverError> {
        if self.stage != expected {
            return Err(DriverError::WrongStage {
                expected,
                actual: self.stage,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
