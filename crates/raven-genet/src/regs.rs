// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GENET v5 register map, bit fields and computed queue/descriptor offsets.
//!
//! Offsets are bytes from the GENET register window (the Pi 4 maps `0x7d580000` through the
//! `/scb` `ranges` to CPU physical `0xfd580000`). Descriptors are device-internal MMIO
//! registers, not RAM: only packet buffers are DMA memory.

/// System revision register.
pub const SYS_REV_CTRL: usize = 0x000;
/// System port control register.
pub const SYS_PORT_CTRL: usize = 0x004;
/// Receive-buffer flush control register.
pub const SYS_RBUF_FLUSH_CTRL: usize = 0x008;
/// Transmit-buffer flush control register.
pub const SYS_TBUF_FLUSH_CTRL: usize = 0x00c;
/// RGMII out-of-band control register.
pub const EXT_RGMII_OOB_CTRL: usize = 0x08c;

/// Level-2 interrupt status.
pub const INTRL2_CPU_STAT: usize = 0x200;
/// Level-2 interrupt clear.
pub const INTRL2_CPU_CLEAR: usize = 0x208;
/// Level-2 interrupt status mask.
pub const INTRL2_CPU_STAT_MASK: usize = 0x20c;
/// Level-2 interrupt set mask.
pub const INTRL2_CPU_SET_MASK: usize = 0x210;
/// Level-2 interrupt clear mask.
pub const INTRL2_CPU_CLEAR_MASK: usize = 0x214;

/// Receive-buffer control.
pub const RBUF_CTRL: usize = 0x300;
/// Receive-buffer `TBUF` size control.
pub const RBUF_TBUF_SIZE_CTRL: usize = 0x3b4;

/// UMAC command register.
pub const UMAC_CMD: usize = 0x808;
/// UMAC MAC address, first four bytes.
pub const UMAC_MAC0: usize = 0x80c;
/// UMAC MAC address, last two bytes.
pub const UMAC_MAC1: usize = 0x810;
/// UMAC maximum frame length.
pub const UMAC_MAX_FRAME_LEN: usize = 0x814;
/// UMAC transmit FIFO flush.
pub const UMAC_TX_FLUSH: usize = 0xb34;
/// UMAC MIB control.
pub const UMAC_MIB_CTRL: usize = 0xd80;

/// MDIO command register.
pub const MDIO_CMD: usize = 0xe14;
/// UMAC multicast destination filter control.
pub const UMAC_MDF_CTRL: usize = 0xe50;
/// First word of multicast filter slot `slot`.
pub const UMAC_MDF_ADDR0: usize = 0xe54;
/// Second word of multicast filter slot `slot`.
pub const UMAC_MDF_ADDR1: usize = 0xe58;

/// Largest GENET register window on the Pi 4, in bytes.
pub const WINDOW_SIZE: usize = 0x1_0000;
/// Size of one hardware descriptor in bytes.
pub const DESC_SIZE: usize = 12;
/// Number of hardware descriptors in each ring.
pub const DESC_COUNT: u16 = 256;
/// Default queue programmed by the Pi 4 driver.
pub const DEFAULT_QUEUE: u8 = 16;
/// Distance between per-queue register blocks.
pub const RING_STRIDE: usize = 0x40;
/// First byte of the RX register block.
pub const RX_BASE: usize = 0x2000;
/// First byte of the TX register block.
pub const TX_BASE: usize = 0x4000;
/// Offset of the per-queue register area inside each direction block.
pub const RING_BLOCK_OFFSET: usize = 0xc00;
/// Offset of the shared DMA control area inside each direction block.
pub const DMA_CONTROL_OFFSET: usize = 0x1040;
/// Number of hardware multicast filter slots.
pub const MDF_FILTERS: u8 = 17;

/// `SYS_REV_CTRL` major-version mask.
pub const SYS_REV_MAJOR_MASK: u32 = 0x0f00_0000;
/// `SYS_REV_CTRL` major-version shift.
pub const SYS_REV_MAJOR_SHIFT: u32 = 24;
/// `SYS_REV_CTRL` minor-version mask.
pub const SYS_REV_MINOR_MASK: u32 = 0x000f_0000;
/// `SYS_REV_CTRL` minor-version shift.
pub const SYS_REV_MINOR_SHIFT: u32 = 16;

/// `SYS_PORT_CTRL`: external GPHY mode.
pub const SYS_PORT_MODE_EXT_GPHY: u32 = 3;
/// `SYS_RBUF_FLUSH_CTRL`: reset the receive buffer.
pub const SYS_RBUF_FLUSH_RESET: u32 = 1 << 1;

/// `EXT_RGMII_OOB_CTRL`: disable the ID mode (plain RGMII).
pub const EXT_RGMII_OOB_ID_MODE_DISABLE: u32 = 1 << 16;
/// `EXT_RGMII_OOB_CTRL`: enable the RGMII interface.
pub const EXT_RGMII_OOB_RGMII_MODE_EN: u32 = 1 << 6;
/// `EXT_RGMII_OOB_CTRL`: disable out-of-band signalling.
pub const EXT_RGMII_OOB_OOB_DISABLE: u32 = 1 << 5;
/// `EXT_RGMII_OOB_CTRL`: report the link through the in-band path.
pub const EXT_RGMII_OOB_RGMII_LINK: u32 = 1 << 4;

/// Interrupt: MDIO error.
pub const IRQ_MDIO_ERROR: u32 = 1 << 24;
/// Interrupt: MDIO transaction done.
pub const IRQ_MDIO_DONE: u32 = 1 << 23;
/// Interrupt: TX DMA done.
pub const IRQ_TXDMA_DONE: u32 = 1 << 16;
/// Interrupt: RX DMA done.
pub const IRQ_RXDMA_DONE: u32 = 1 << 13;

/// `RBUF_CTRL`: disable bad-frame reception.
pub const RBUF_BAD_DIS: u32 = 1 << 2;
/// `RBUF_CTRL`: two-byte receive alignment.
pub const RBUF_ALIGN_2B: u32 = 1 << 1;
/// `RBUF_CTRL`: 64-byte receive buffers.
pub const RBUF_64B_EN: u32 = 1 << 0;

/// `UMAC_CMD`: local loopback.
pub const UMAC_CMD_LCL_LOOP_EN: u32 = 1 << 15;
/// `UMAC_CMD`: software reset.
pub const UMAC_CMD_SW_RESET: u32 = 1 << 13;
/// `UMAC_CMD`: promiscuous mode.
pub const UMAC_CMD_PROMISC: u32 = 1 << 4;
/// `UMAC_CMD`: speed field mask.
pub const UMAC_CMD_SPEED_MASK: u32 = 0x0000_000c;
/// `UMAC_CMD`: speed field shift.
pub const UMAC_CMD_SPEED_SHIFT: u32 = 2;
/// `UMAC_CMD`: 10 Mbit/s.
pub const UMAC_CMD_SPEED_10: u32 = 0;
/// `UMAC_CMD`: 100 Mbit/s.
pub const UMAC_CMD_SPEED_100: u32 = 1;
/// `UMAC_CMD`: 1000 Mbit/s.
pub const UMAC_CMD_SPEED_1000: u32 = 2;
/// `UMAC_CMD`: enable the receiver.
pub const UMAC_CMD_RXEN: u32 = 1 << 1;
/// `UMAC_CMD`: enable the transmitter.
pub const UMAC_CMD_TXEN: u32 = 1 << 0;

/// `UMAC_MIB_CTRL`: reset the TX MIB counters.
pub const UMAC_MIB_RESET_TX: u32 = 1 << 2;
/// `UMAC_MIB_CTRL`: reset the runt counters.
pub const UMAC_MIB_RESET_RUNT: u32 = 1 << 1;
/// `UMAC_MIB_CTRL`: reset the RX MIB counters.
pub const UMAC_MIB_RESET_RX: u32 = 1 << 0;

/// `MDIO_CMD`: transaction busy.
pub const MDIO_START_BUSY: u32 = 1 << 29;
/// `MDIO_CMD`: read transaction.
pub const MDIO_READ: u32 = 1 << 27;
/// `MDIO_CMD`: write transaction.
pub const MDIO_WRITE: u32 = 1 << 26;
/// `MDIO_CMD`: PHY address mask.
pub const MDIO_PMD_MASK: u32 = 0x03e0_0000;
/// `MDIO_CMD`: PHY address shift.
pub const MDIO_PMD_SHIFT: u32 = 21;
/// `MDIO_CMD`: register mask.
pub const MDIO_REG_MASK: u32 = 0x001f_0000;
/// `MDIO_CMD`: register shift.
pub const MDIO_REG_SHIFT: u32 = 16;
/// `MDIO_CMD`: data mask.
pub const MDIO_DATA_MASK: u32 = 0x0000_ffff;

/// RX ring descriptor count field mask in `RING_BUF_SIZE`.
pub const RING_BUF_SIZE_DESC_COUNT_MASK: u32 = 0xffff_0000;
/// RX ring buffer length field mask in `RING_BUF_SIZE`.
pub const RING_BUF_SIZE_BUF_LENGTH_MASK: u32 = 0x0000_ffff;

/// Interrupt mitigation threshold field mask.
pub const INTR_THRESHOLD_MASK: u32 = 0x0000_01ff;
/// DMA ring timeout field mask.
pub const DMA_RING_TIMEOUT_MASK: u32 = 0x0000_ffff;

/// Raw major version from `SYS_REV_CTRL`.
#[must_use]
pub const fn revision_major(rev: u32) -> u8 {
    ((rev & SYS_REV_MAJOR_MASK) >> SYS_REV_MAJOR_SHIFT) as u8
}

/// Raw minor version from `SYS_REV_CTRL`.
#[must_use]
pub const fn revision_minor(rev: u32) -> u8 {
    ((rev & SYS_REV_MINOR_MASK) >> SYS_REV_MINOR_SHIFT) as u8
}

/// Normalised major version.
///
/// The block reports raw 6 on the Pi 4 GENET v5; raw 5 is the older v4 numbering. The NetBSD
/// driver maps raw 0 to 1 and both raw 5 and raw 6 down by one before comparing with 5.
#[must_use]
pub const fn normalized_major(rev: u32) -> u8 {
    match revision_major(rev) {
        0 => 1,
        5 | 6 => revision_major(rev) - 1,
        major => major,
    }
}

/// True when the revision register identifies a GENET v5 block.
#[must_use]
pub const fn revision_is_v5(rev: u32) -> bool {
    normalized_major(rev) == 5
}

/// Base of the register block for `queue`.
#[must_use]
pub const fn rx_ring_base(queue: u8) -> Option<usize> {
    if queue > DEFAULT_QUEUE {
        return None;
    }
    Some(RX_BASE + RING_BLOCK_OFFSET + RING_STRIDE * queue as usize)
}

/// Base of the TX register block for `queue`.
#[must_use]
pub const fn tx_ring_base(queue: u8) -> Option<usize> {
    if queue > DEFAULT_QUEUE {
        return None;
    }
    Some(TX_BASE + RING_BLOCK_OFFSET + RING_STRIDE * queue as usize)
}

/// `RX_DMA_WRITE_PTR_LO(queue)`.
#[must_use]
pub fn rx_dma_write_ptr_lo(queue: u8) -> Option<usize> {
    rx_ring_base(queue)
}

/// `RX_DMA_WRITE_PTR_HI(queue)`.
#[must_use]
pub fn rx_dma_write_ptr_hi(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x04)
}

/// `RX_DMA_PROD_INDEX(queue)`.
#[must_use]
pub fn rx_dma_prod_index(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x08)
}

/// `RX_DMA_CONS_INDEX(queue)`.
#[must_use]
pub fn rx_dma_cons_index(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x0c)
}

/// `RX_DMA_RING_BUF_SIZE(queue)`.
#[must_use]
pub fn rx_dma_ring_buf_size(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x10)
}

/// `RX_DMA_START_ADDR_LO(queue)`.
#[must_use]
pub fn rx_dma_start_addr_lo(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x14)
}

/// `RX_DMA_START_ADDR_HI(queue)`.
#[must_use]
pub fn rx_dma_start_addr_hi(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x18)
}

/// `RX_DMA_END_ADDR_LO(queue)`.
#[must_use]
pub fn rx_dma_end_addr_lo(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x1c)
}

/// `RX_DMA_END_ADDR_HI(queue)`.
#[must_use]
pub fn rx_dma_end_addr_hi(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x20)
}

/// `RX_DMA_MBUF_DONE_THRES(queue)`.
#[must_use]
pub fn rx_dma_mbuf_done_thres(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x24)
}

/// `RX_DMA_XON_XOFF_THRES(queue)`.
#[must_use]
pub fn rx_dma_xon_xoff_thres(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x28)
}

/// `RX_DMA_READ_PTR_LO(queue)`.
#[must_use]
pub fn rx_dma_read_ptr_lo(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x2c)
}

/// `RX_DMA_READ_PTR_HI(queue)`.
#[must_use]
pub fn rx_dma_read_ptr_hi(queue: u8) -> Option<usize> {
    Some(rx_ring_base(queue)? + 0x30)
}

/// `TX_DMA_READ_PTR_LO(queue)`.
#[must_use]
pub fn tx_dma_read_ptr_lo(queue: u8) -> Option<usize> {
    tx_ring_base(queue)
}

/// `TX_DMA_READ_PTR_HI(queue)`.
#[must_use]
pub fn tx_dma_read_ptr_hi(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x04)
}

/// `TX_DMA_CONS_INDEX(queue)`.
#[must_use]
pub fn tx_dma_cons_index(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x08)
}

/// `TX_DMA_PROD_INDEX(queue)`.
#[must_use]
pub fn tx_dma_prod_index(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x0c)
}

/// `TX_DMA_RING_BUF_SIZE(queue)`.
#[must_use]
pub fn tx_dma_ring_buf_size(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x10)
}

/// `TX_DMA_START_ADDR_LO(queue)`.
#[must_use]
pub fn tx_dma_start_addr_lo(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x14)
}

/// `TX_DMA_START_ADDR_HI(queue)`.
#[must_use]
pub fn tx_dma_start_addr_hi(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x18)
}

/// `TX_DMA_END_ADDR_LO(queue)`.
#[must_use]
pub fn tx_dma_end_addr_lo(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x1c)
}

/// `TX_DMA_END_ADDR_HI(queue)`.
#[must_use]
pub fn tx_dma_end_addr_hi(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x20)
}

/// `TX_DMA_MBUF_DONE_THRES(queue)`.
#[must_use]
pub fn tx_dma_mbuf_done_thres(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x24)
}

/// `TX_DMA_FLOW_PERIOD(queue)`.
#[must_use]
pub fn tx_dma_flow_period(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x28)
}

/// `TX_DMA_WRITE_PTR_LO(queue)`.
#[must_use]
pub fn tx_dma_write_ptr_lo(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x2c)
}

/// `TX_DMA_WRITE_PTR_HI(queue)`.
#[must_use]
pub fn tx_dma_write_ptr_hi(queue: u8) -> Option<usize> {
    Some(tx_ring_base(queue)? + 0x30)
}

/// RX DMA ring configuration register.
pub const RX_DMA_RING_CFG: usize = RX_BASE + DMA_CONTROL_OFFSET;
/// RX DMA control register.
pub const RX_DMA_CTRL: usize = RX_BASE + DMA_CONTROL_OFFSET + 0x04;
/// RX DMA control: enable the DMA engine.
pub const RX_DMA_CTRL_EN: u32 = 1 << 0;
/// RX DMA control: enable receive buffer `queue` (`queue + 1` bit).
#[must_use]
pub fn rx_dma_ctrl_rbuf_en(queue: u8) -> u32 {
    1 << (queue as u32 + 1)
}
/// RX DMA SCB burst size register.
pub const RX_SCB_BURST_SIZE: usize = RX_BASE + DMA_CONTROL_OFFSET + 0x0c;
/// RX DMA ring timeout register for `queue`.
#[must_use]
pub fn rx_dma_ring_timeout(queue: u8) -> Option<usize> {
    if queue > DEFAULT_QUEUE {
        return None;
    }
    Some(RX_BASE + DMA_CONTROL_OFFSET + 0x2c + 4 * queue as usize)
}

/// TX DMA ring configuration register.
pub const TX_DMA_RING_CFG: usize = TX_BASE + DMA_CONTROL_OFFSET;
/// TX DMA control register.
pub const TX_DMA_CTRL: usize = TX_BASE + DMA_CONTROL_OFFSET + 0x04;
/// TX DMA control: enable the DMA engine.
pub const TX_DMA_CTRL_EN: u32 = 1 << 0;
/// TX DMA control: enable transmit buffer `queue` (`queue + 1` bit).
#[must_use]
pub fn tx_dma_ctrl_rbuf_en(queue: u8) -> u32 {
    1 << (queue as u32 + 1)
}
/// TX DMA SCB burst size register.
pub const TX_SCB_BURST_SIZE: usize = TX_BASE + DMA_CONTROL_OFFSET + 0x0c;
/// TX DMA ring timeout register for `queue`.
#[must_use]
pub fn tx_dma_ring_timeout(queue: u8) -> Option<usize> {
    if queue > DEFAULT_QUEUE {
        return None;
    }
    Some(TX_BASE + DMA_CONTROL_OFFSET + 0x2c + 4 * queue as usize)
}

/// Address of the RX descriptor status word for `index`.
#[must_use]
pub const fn rx_desc_status(index: u16) -> Option<usize> {
    if index >= DESC_COUNT {
        return None;
    }
    Some(RX_BASE + DESC_SIZE * index as usize)
}

/// Address of the RX descriptor address-low word for `index`.
#[must_use]
pub fn rx_desc_address_lo(index: u16) -> Option<usize> {
    Some(rx_desc_status(index)? + 0x04)
}

/// Address of the RX descriptor address-high word for `index`.
#[must_use]
pub fn rx_desc_address_hi(index: u16) -> Option<usize> {
    Some(rx_desc_status(index)? + 0x08)
}

/// Address of the TX descriptor status word for `index`.
#[must_use]
pub const fn tx_desc_status(index: u16) -> Option<usize> {
    if index >= DESC_COUNT {
        return None;
    }
    Some(TX_BASE + DESC_SIZE * index as usize)
}

/// Address of the TX descriptor address-low word for `index`.
#[must_use]
pub fn tx_desc_address_lo(index: u16) -> Option<usize> {
    Some(tx_desc_status(index)? + 0x04)
}

/// Address of the TX descriptor address-high word for `index`.
#[must_use]
pub fn tx_desc_address_hi(index: u16) -> Option<usize> {
    Some(tx_desc_status(index)? + 0x08)
}

/// Address of multicast filter slot `slot`'s first word.
#[must_use]
pub fn umac_mdf_addr0(slot: u8) -> Option<usize> {
    if slot >= MDF_FILTERS {
        return None;
    }
    Some(UMAC_MDF_ADDR0 + 8 * slot as usize)
}

/// Address of multicast filter slot `slot`'s second word.
#[must_use]
pub fn umac_mdf_addr1(slot: u8) -> Option<usize> {
    if slot >= MDF_FILTERS {
        return None;
    }
    Some(UMAC_MDF_ADDR1 + 8 * slot as usize)
}

/// Encode `RING_BUF_SIZE`: descriptor count in the high half, buffer length in the low half.
#[must_use]
pub const fn ring_buf_size(desc_count: u16, buffer_len: u16) -> u32 {
    ((desc_count as u32) << 16) | buffer_len as u32
}

/// Descriptor count from a `RING_BUF_SIZE` value.
#[must_use]
pub const fn ring_desc_count(value: u32) -> u16 {
    (value >> 16) as u16
}

/// Buffer length from a `RING_BUF_SIZE` value.
#[must_use]
pub const fn ring_buffer_len(value: u32) -> u16 {
    value as u16
}

/// End-address register value: the last 32-bit word of the descriptor array.
#[must_use]
pub const fn ring_end_address(desc_count: u16) -> u32 {
    (desc_count as u32 * DESC_SIZE as u32 / 4) - 1
}

/// RX xon/xoff threshold value: xon in the high half, xoff in the low half.
#[must_use]
pub const fn rx_xon_xoff(xon: u16, xoff: u16) -> u32 {
    ((xon as u32) << 16) | xoff as u32
}

/// Convert a microsecond delay to the GENET 125 MHz/1024 tick domain.
#[must_use]
pub const fn microseconds_to_ticks(microseconds: u32) -> u32 {
    microseconds.saturating_mul(125).div_ceil(1024)
}

#[cfg(test)]
#[path = "regs_ut.rs"]
mod tests;
