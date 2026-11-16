// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Board seam over `raven-driver` MMIO and DMA regions.
//!
//! The real driver owns one MMIO window and one DMA window granted by the supervisor. The DMA
//! window is split into an RX pool and a TX pool; each descriptor names a 2 KiB slot and the
//! device sees the DMA window's bus base plus the slot offset. The host tests build the same
//! object with leaked local buffers and a fake bus base, so bounds and alignment are exercised
//! without a board.

use crate::{Buffer, Seam, SeamError};
use raven_driver::{AccessError, DmaRegion, MmioRegion};
use raven_genet::regs;

/// Fixed Pi 4 packet-buffer size.
pub const PACKET_BUFFER_LEN: usize = 2048;
/// Alignment of every packet buffer in the DMA window.
pub const PACKET_ALIGN: usize = 64;

/// The RX/TX pool layout inside one DMA window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformLayout {
    /// Byte offset of the RX pool.
    pub rx_offset: usize,
    /// Byte offset of the TX pool.
    pub tx_offset: usize,
    /// Number of RX slots.
    pub rx_count: usize,
    /// Number of TX slots.
    pub tx_count: usize,
    /// Bytes in one slot.
    pub buffer_len: usize,
    /// Distance between slot starts.
    pub stride: usize,
}

impl PlatformLayout {
    /// The 256+256 ring layout: 512 KiB RX followed by 512 KiB TX.
    #[must_use]
    pub const fn pi4_default() -> Self {
        Self {
            rx_offset: 0,
            tx_offset: 256 * PACKET_BUFFER_LEN,
            rx_count: regs::DESC_COUNT as usize,
            tx_count: regs::DESC_COUNT as usize,
            buffer_len: PACKET_BUFFER_LEN,
            stride: PACKET_BUFFER_LEN,
        }
    }

    /// Validate the layout against one DMA window length.
    pub fn validate(self, region_len: usize) -> Result<(), SeamError> {
        if self.buffer_len == 0
            || self.buffer_len > PACKET_BUFFER_LEN
            || self.stride < self.buffer_len
            || self.rx_count == 0
            || self.tx_count == 0
            || self.rx_count > regs::DESC_COUNT as usize
            || self.tx_count > regs::DESC_COUNT as usize
        {
            return Err(SeamError::Bounds);
        }
        if !self.rx_offset.is_multiple_of(PACKET_ALIGN)
            || !self.tx_offset.is_multiple_of(PACKET_ALIGN)
            || !self.stride.is_multiple_of(PACKET_ALIGN)
        {
            return Err(SeamError::Alignment);
        }
        let rx_end = self
            .rx_offset
            .checked_add(self.rx_count.saturating_mul(self.stride))
            .ok_or(SeamError::Bounds)?;
        let tx_end = self
            .tx_offset
            .checked_add(self.tx_count.saturating_mul(self.stride))
            .ok_or(SeamError::Bounds)?;
        if rx_end > region_len || tx_end > region_len {
            return Err(SeamError::Bounds);
        }
        if rx_end > self.tx_offset && tx_end > self.rx_offset {
            return Err(SeamError::Bounds);
        }
        Ok(())
    }

    /// Device-visible address of RX slot `index`.
    #[must_use]
    pub fn rx_bus_address(self, base: u64, index: usize) -> Option<u64> {
        if index >= self.rx_count {
            return None;
        }
        base.checked_add(self.rx_offset as u64)?
            .checked_add((index.checked_mul(self.stride)?) as u64)
    }

    /// Device-visible address of TX slot `index`.
    #[must_use]
    pub fn tx_bus_address(self, base: u64, index: usize) -> Option<u64> {
        if index >= self.tx_count {
            return None;
        }
        base.checked_add(self.tx_offset as u64)?
            .checked_add((index.checked_mul(self.stride)?) as u64)
    }
}

/// The real board seam.
pub struct PlatformSeam {
    mmio: MmioRegion,
    dma: DmaRegion,
    layout: PlatformLayout,
}

impl PlatformSeam {
    /// Wrap mapped MMIO/DMA regions after validating the pool layout.
    pub fn new(
        mmio: MmioRegion,
        dma: DmaRegion,
        layout: PlatformLayout,
    ) -> Result<Self, SeamError> {
        layout.validate(dma.len())?;
        Ok(Self { mmio, dma, layout })
    }

    /// The pool layout.
    #[must_use]
    pub const fn layout(&self) -> PlatformLayout {
        self.layout
    }

    /// The DMA window length.
    #[must_use]
    pub fn dma_len(&self) -> usize {
        self.dma.len()
    }

    /// Device-visible base of the DMA window.
    #[must_use]
    pub fn dma_bus_address(&self) -> u64 {
        self.dma.bus_address()
    }

    /// Copy `out.len()` bytes at `offset` out of the DMA window.
    pub fn read_dma(&self, offset: usize, out: &mut [u8]) -> Result<(), SeamError> {
        self.dma.read_bytes(offset, out).map_err(map_access)
    }

    /// Copy `data` into the DMA window at `offset`.
    pub fn write_dma(&mut self, offset: usize, data: &[u8]) -> Result<(), SeamError> {
        self.dma.write_bytes(offset, data).map_err(map_access)
    }

    /// Map both regions from `raven_rt` handles.
    #[cfg(target_os = "none")]
    pub fn from_handles(
        mmio_handle: raven_abi::types::Handle,
        mmio_len: usize,
        dma_handle: raven_abi::types::Handle,
        dma_len: usize,
        layout: PlatformLayout,
    ) -> Result<Self, SeamError> {
        let dma_info = raven_rt::dma_info(dma_handle).map_err(|_| SeamError::Io)?;
        if dma_info.len < dma_len as u64 {
            return Err(SeamError::Bounds);
        }
        let mmio_window =
            raven_rt::map_mmio_mut(mmio_handle, mmio_len).map_err(|_| SeamError::Bounds)?;
        let dma_window =
            raven_rt::map_mmio_mut(dma_handle, dma_len).map_err(|_| SeamError::Bounds)?;
        Self::new(
            MmioRegion::new(mmio_window),
            DmaRegion::new(dma_window, dma_info.bus_addr),
            layout,
        )
    }
}

impl Seam for PlatformSeam {
    fn read32(&mut self, offset: usize) -> Result<u32, SeamError> {
        self.mmio.read_u32(offset).map_err(map_access)
    }

    fn write32(&mut self, offset: usize, value: u32) -> Result<(), SeamError> {
        self.mmio.write_u32(offset, value).map_err(map_access)
    }

    fn rx_buffer(&mut self, index: usize) -> Option<Buffer> {
        Some(Buffer {
            bus_addr: self.layout.rx_bus_address(self.dma.bus_address(), index)?,
            len: self.layout.buffer_len,
        })
    }

    fn tx_buffer(&mut self, index: usize) -> Option<Buffer> {
        Some(Buffer {
            bus_addr: self.layout.tx_bus_address(self.dma.bus_address(), index)?,
            len: self.layout.buffer_len,
        })
    }

    fn delay_us(&mut self, microseconds: u32) {
        #[cfg(target_os = "none")]
        {
            let _ = raven_rt::nanosleep(u64::from(microseconds) * 1000);
        }
        #[cfg(not(target_os = "none"))]
        {
            let _ = microseconds;
        }
    }
}

fn map_access(error: AccessError) -> SeamError {
    match error {
        AccessError::Bounds => SeamError::Bounds,
        AccessError::Alignment => SeamError::Alignment,
    }
}

#[cfg(test)]
#[path = "platform_ut.rs"]
mod tests;
