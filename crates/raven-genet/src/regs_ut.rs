// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the GENET v5 register map.

use super::*;

#[test]
fn revision_fields_and_normalisation_match_the_pi4_block() {
    let rev = (6 << SYS_REV_MAJOR_SHIFT) | (2 << SYS_REV_MINOR_SHIFT);
    assert_eq!(revision_major(rev), 6);
    assert_eq!(revision_minor(rev), 2);
    assert_eq!(normalized_major(rev), 5);
    assert!(revision_is_v5(rev));
}

#[test]
fn raw_revision_five_is_the_older_numbering() {
    let rev = 5 << SYS_REV_MAJOR_SHIFT;
    assert_eq!(normalized_major(rev), 4);
    assert!(!revision_is_v5(rev));
}

#[test]
fn zero_major_revision_is_not_a_genet_v5() {
    assert_eq!(normalized_major(0), 1);
    assert!(!revision_is_v5(0));
}

#[test]
fn unknown_major_revisions_are_rejected() {
    assert_eq!(normalized_major(3 << SYS_REV_MAJOR_SHIFT), 3);
    assert!(!revision_is_v5(3 << SYS_REV_MAJOR_SHIFT));
}

#[test]
fn the_system_register_group_offsets_are_stable() {
    assert_eq!(SYS_REV_CTRL, 0x000);
    assert_eq!(SYS_PORT_CTRL, 0x004);
    assert_eq!(EXT_RGMII_OOB_CTRL, 0x08c);
    assert_eq!(INTRL2_CPU_STAT, 0x200);
    assert_eq!(INTRL2_CPU_CLEAR_MASK, 0x214);
    assert_eq!(RBUF_CTRL, 0x300);
    assert_eq!(UMAC_CMD, 0x808);
    assert_eq!(UMAC_MAC0, 0x80c);
    assert_eq!(UMAC_MAC1, 0x810);
    assert_eq!(MDIO_CMD, 0xe14);
}

#[test]
fn queue_zero_and_the_default_queue_have_known_bases() {
    assert_eq!(rx_ring_base(0), Some(0x2c00));
    assert_eq!(tx_ring_base(0), Some(0x4c00));
    assert_eq!(rx_ring_base(DEFAULT_QUEUE), Some(0x3000));
    assert_eq!(tx_ring_base(DEFAULT_QUEUE), Some(0x5000));
}

#[test]
fn queues_above_the_default_are_rejected() {
    assert_eq!(rx_ring_base(DEFAULT_QUEUE + 1), None);
    assert_eq!(tx_ring_base(DEFAULT_QUEUE + 1), None);
    assert_eq!(rx_dma_ring_timeout(DEFAULT_QUEUE + 1), None);
    assert_eq!(tx_dma_ring_timeout(DEFAULT_QUEUE + 1), None);
}

#[test]
fn every_queue_register_is_the_base_plus_its_documented_offset() {
    let queue = DEFAULT_QUEUE;
    let base = rx_ring_base(queue).expect("queue base");
    assert_eq!(rx_dma_write_ptr_lo(queue), Some(base));
    assert_eq!(rx_dma_write_ptr_hi(queue), Some(base + 0x04));
    assert_eq!(rx_dma_prod_index(queue), Some(base + 0x08));
    assert_eq!(rx_dma_cons_index(queue), Some(base + 0x0c));
    assert_eq!(rx_dma_ring_buf_size(queue), Some(base + 0x10));
    assert_eq!(rx_dma_start_addr_lo(queue), Some(base + 0x14));
    assert_eq!(rx_dma_start_addr_hi(queue), Some(base + 0x18));
    assert_eq!(rx_dma_end_addr_lo(queue), Some(base + 0x1c));
    assert_eq!(rx_dma_end_addr_hi(queue), Some(base + 0x20));
    assert_eq!(rx_dma_mbuf_done_thres(queue), Some(base + 0x24));
    assert_eq!(rx_dma_xon_xoff_thres(queue), Some(base + 0x28));
    assert_eq!(rx_dma_read_ptr_lo(queue), Some(base + 0x2c));
    assert_eq!(rx_dma_read_ptr_hi(queue), Some(base + 0x30));
}

#[test]
fn every_tx_queue_register_is_the_base_plus_its_documented_offset() {
    let queue = DEFAULT_QUEUE;
    let base = tx_ring_base(queue).expect("queue base");
    assert_eq!(tx_dma_read_ptr_lo(queue), Some(base));
    assert_eq!(tx_dma_read_ptr_hi(queue), Some(base + 0x04));
    assert_eq!(tx_dma_cons_index(queue), Some(base + 0x08));
    assert_eq!(tx_dma_prod_index(queue), Some(base + 0x0c));
    assert_eq!(tx_dma_ring_buf_size(queue), Some(base + 0x10));
    assert_eq!(tx_dma_start_addr_lo(queue), Some(base + 0x14));
    assert_eq!(tx_dma_start_addr_hi(queue), Some(base + 0x18));
    assert_eq!(tx_dma_end_addr_lo(queue), Some(base + 0x1c));
    assert_eq!(tx_dma_end_addr_hi(queue), Some(base + 0x20));
    assert_eq!(tx_dma_mbuf_done_thres(queue), Some(base + 0x24));
    assert_eq!(tx_dma_flow_period(queue), Some(base + 0x28));
    assert_eq!(tx_dma_write_ptr_lo(queue), Some(base + 0x2c));
    assert_eq!(tx_dma_write_ptr_hi(queue), Some(base + 0x30));
}

#[test]
fn descriptors_are_twelve_bytes_apart_in_device_memory() {
    assert_eq!(rx_desc_status(0), Some(0x2000));
    assert_eq!(rx_desc_address_lo(0), Some(0x2004));
    assert_eq!(rx_desc_address_hi(0), Some(0x2008));
    assert_eq!(rx_desc_status(1), Some(0x200c));
    assert_eq!(rx_desc_status(255), Some(0x2bf4));
    assert_eq!(rx_desc_status(256), None);
    assert_eq!(tx_desc_status(255), Some(0x4bf4));
    assert_eq!(tx_desc_status(256), None);
}

#[test]
fn ring_buffer_size_encodes_count_and_length() {
    let value = ring_buf_size(DESC_COUNT, 2048);
    assert_eq!(value, 0x0100_0800);
    assert_eq!(ring_desc_count(value), DESC_COUNT);
    assert_eq!(ring_buffer_len(value), 2048);
}

#[test]
fn ring_end_address_names_the_last_descriptor_word() {
    assert_eq!(ring_end_address(DESC_COUNT), 767);
    assert_eq!(ring_end_address(1), 2);
}

#[test]
fn xon_xoff_thresholds_use_the_documented_halves() {
    assert_eq!(rx_xon_xoff(5, 16), 0x0005_0010);
}

#[test]
fn microsecond_conversion_rounds_up() {
    assert_eq!(microseconds_to_ticks(0), 0);
    assert_eq!(microseconds_to_ticks(1), 1);
    assert_eq!(microseconds_to_ticks(57), 7);
    assert_eq!(microseconds_to_ticks(1024), 125);
}

#[test]
fn multicast_filter_slots_are_eight_bytes_apart_and_bounded() {
    assert_eq!(umac_mdf_addr0(0), Some(0xe54));
    assert_eq!(umac_mdf_addr1(0), Some(0xe58));
    assert_eq!(umac_mdf_addr0(16), Some(0xed4));
    assert_eq!(umac_mdf_addr1(16), Some(0xed8));
    assert_eq!(umac_mdf_addr0(MDF_FILTERS), None);
    assert_eq!(umac_mdf_addr1(MDF_FILTERS), None);
}

#[test]
fn interrupt_and_control_bits_match_the_register_map() {
    assert_eq!(IRQ_MDIO_ERROR, 1 << 24);
    assert_eq!(IRQ_MDIO_DONE, 1 << 23);
    assert_eq!(IRQ_TXDMA_DONE, 1 << 16);
    assert_eq!(IRQ_RXDMA_DONE, 1 << 13);
    assert_eq!(RBUF_ALIGN_2B, 1 << 1);
    assert_eq!(UMAC_CMD_SPEED_1000, 2);
    assert_eq!(UMAC_CMD_SPEED_MASK, 0x0000_000c);
    assert_eq!(MDIO_START_BUSY, 1 << 29);
    assert_eq!(MDIO_READ, 1 << 27);
    assert_eq!(MDIO_WRITE, 1 << 26);
    assert_eq!(MDIO_PMD_MASK, 0x03e0_0000);
    assert_eq!(MDIO_PMD_SHIFT, 21);
    assert_eq!(MDIO_REG_MASK, 0x001f_0000);
    assert_eq!(MDIO_REG_SHIFT, 16);
    assert_eq!(MDIO_DATA_MASK, 0x0000_ffff);
}

#[test]
fn the_dma_control_and_timeout_registers_are_reachable() {
    assert_eq!(RX_DMA_RING_CFG, 0x3040);
    assert_eq!(RX_DMA_CTRL, 0x3044);
    assert_eq!(TX_DMA_RING_CFG, 0x5040);
    assert_eq!(TX_DMA_CTRL, 0x5044);
    assert_eq!(rx_dma_ring_timeout(16), Some(0x30ac));
    assert_eq!(tx_dma_ring_timeout(16), Some(0x50ac));
    assert_eq!(rx_dma_ctrl_rbuf_en(16), 1 << 17);
    assert_eq!(tx_dma_ctrl_rbuf_en(16), 1 << 17);
}
