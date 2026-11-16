// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the DMA control-block ring.

use super::*;

#[test]
fn an_audio_control_block_encodes_the_documented_words() {
    let block = ControlBlock::audio(0x0010_0000, 8192, 0x0010_0020);
    assert_eq!(block.ti, 0x0405_2149);
    assert_eq!(block.dest, 0x7e20_c818);
    assert_eq!(block.stride, 0);
    let bytes = block.encode();
    assert_eq!(bytes.len(), 32);
    assert_eq!(&bytes[0..4], &0x0405_2149u32.to_le_bytes());
    assert_eq!(&bytes[4..8], &0x0010_0000u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &0x7e20_c818u32.to_le_bytes());
    assert_eq!(&bytes[12..16], &8192u32.to_le_bytes());
    assert_eq!(&bytes[16..20], &0u32.to_le_bytes());
    assert_eq!(&bytes[20..24], &0x0010_0020u32.to_le_bytes());
    assert_eq!(&bytes[24..32], &[0u8; 8]);
}

#[test]
fn a_control_block_round_trips() {
    let block = ControlBlock::audio(0x0100_0000, 4096, 0x0100_0040);
    assert_eq!(ControlBlock::decode(&block.encode()), Some(block));
    assert_eq!(ControlBlock::decode(&[0u8; 23]), None);
}

#[test]
fn a_ring_wraps_its_duty_buffer_and_next_pointer() {
    let ring = RingGeometry::new(0x1000_0000, 0x10000, 0, 0x8000, 1024, 4, 5).expect("ring");
    assert_eq!(ring.period_bytes(), 8192);
    assert_eq!(ring.duty_bytes(), 32 * 1024);
    assert_eq!(ring.cb_bytes(), 128);
    assert_eq!(ring.duty_bus(0), Some(0x1000_8000));
    assert_eq!(ring.duty_bus(3), Some(0x1000_e000));
    assert_eq!(ring.cb_bus(0), Some(0x1000_0000));
    assert_eq!(ring.cb_bus(3), Some(0x1000_0060));
    let last = ring.control_block(3).expect("last");
    assert_eq!(last.next, 0x1000_0000);
    assert_eq!(last.source, 0x1000_e000);
    assert_eq!(last.length, 8192);
}

#[test]
fn a_ring_rejects_bad_geometry() {
    assert_eq!(
        RingGeometry::new(0x1000_0000, 0x1000, 0, 0, 0, 4, 5),
        Err(DmaError::ZeroPeriod)
    );
    assert_eq!(
        RingGeometry::new(0x1000_0000, 0x1000, 0, 0, 1024, 0, 5),
        Err(DmaError::ZeroPeriod)
    );
    assert_eq!(
        RingGeometry::new(0x1000_0000, 0x1000, 0, 0, 1024, MAX_PERIODS + 1, 5),
        Err(DmaError::TooManyPeriods)
    );
    assert_eq!(
        RingGeometry::new(0x1000_0000, 0x1000, 1, 0, 1024, 4, 5),
        Err(DmaError::Unaligned)
    );
    assert_eq!(
        RingGeometry::new(0x1000_0000, 0x1000, 0, 1, 1024, 4, 5),
        Err(DmaError::Unaligned)
    );
    assert_eq!(
        RingGeometry::new(0x1000_0000, 0x1000, 0, 0, 1024, 4, 15),
        Err(DmaError::InvalidChannel)
    );
    assert_eq!(
        RingGeometry::new(0x1000_0000, 0x1000, 0, 0, 1024, 4, 5),
        Err(DmaError::RegionTooSmall)
    );
}

#[test]
fn a_ring_rejects_address_overflow() {
    assert_eq!(
        RingGeometry::new(u32::MAX - 0x100, 0x3000, 0, 0x1000, 1024, 1, 5),
        Err(DmaError::AddressOverflow)
    );
}

#[test]
fn the_channel_registers_and_enable_bit_are_computed() {
    let ring = RingGeometry::new(0x1000_0000, 0x10000, 0, 0x8000, 1024, 4, 5).expect("ring");
    assert_eq!(ring.channel_base(), Some(0x500));
    assert_eq!(ring.enable_bit(), 1 << 5);
    assert_eq!(RingGeometry::enable_register(), 0xff0);
    assert_eq!(RingGeometry::interrupt_status_register(), 0xfe0);
    assert_eq!(CS_COMPLETION_CLEAR, DMA_CS_INTMASK | DMA_CS_ERROR);
}
