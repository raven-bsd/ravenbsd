// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for SDMA/ADMA2 descriptors, window mapping and bounce decisions.

use super::*;
use crate::fdt::DmaWindow;

const PI4_WINDOW: DmaWindow = DmaWindow {
    bus_base: 0xc000_0000,
    cpu_base: 0,
    size: 0x4000_0000,
};

#[test]
fn descriptor_widths_match_the_sdhci_layout() {
    assert_eq!(DescriptorWidth::Bits32.encoded_len(), 8);
    assert_eq!(DescriptorWidth::Bits64.encoded_len(), 12);
    assert_eq!(DescriptorWidth::Bits32.alignment(), 4);
    assert_eq!(DescriptorWidth::Bits64.alignment(), 8);
    assert_eq!(DescriptorWidth::Bits32.address_limit(), u64::from(u32::MAX));
    assert_eq!(DescriptorWidth::Bits64.address_limit(), u64::MAX);
}

#[test]
fn a_32_bit_transfer_descriptor_round_trips() {
    let descriptor = Adma2Descriptor::transfer(0xc000_1000, 4096, true, true).expect("descriptor");
    assert!(descriptor.valid);
    assert!(descriptor.end);
    assert!(descriptor.interrupt);
    assert_eq!(descriptor.action, DescriptorAction::Transfer);
    assert_eq!(descriptor.attribute(), 0x27);
    assert_eq!(descriptor.length, 4096);
    assert_eq!(descriptor.actual_length(), 4096);

    let bytes = descriptor.encode32().expect("encode");
    assert_eq!(bytes, [0x27, 0x00, 0x00, 0x10, 0x00, 0x10, 0x00, 0xc0]);
    assert_eq!(Adma2Descriptor::decode32(&bytes), Ok(descriptor));
}

#[test]
fn a_64_bit_transfer_descriptor_round_trips_above_four_gib() {
    let descriptor =
        Adma2Descriptor::transfer(0x1_0000_2000, 0x8000, false, false).expect("descriptor");
    let bytes = descriptor.encode64();
    assert_eq!(bytes.len(), ADMA2_DESCRIPTOR64_LEN);
    assert_eq!(bytes[0], 0x21);
    assert_eq!(Adma2Descriptor::decode64(&bytes), Ok(descriptor));
}

#[test]
fn the_65536_byte_length_uses_the_zero_encoding() {
    let descriptor =
        Adma2Descriptor::transfer(0xc000_0000, ADMA2_MAX_LENGTH, true, false).expect("descriptor");
    assert_eq!(descriptor.length, 0);
    assert_eq!(descriptor.actual_length(), ADMA2_MAX_LENGTH);
    assert_eq!(descriptor.encode32().expect("encode")[2..4], [0, 0]);
}

#[test]
fn descriptor_lengths_and_addresses_are_validated() {
    assert_eq!(
        Adma2Descriptor::transfer(0xc000_0000, 0, true, false),
        Err(DmaError::DescriptorLength { length: 0 })
    );
    assert_eq!(
        Adma2Descriptor::transfer(0xc000_0000, ADMA2_MAX_LENGTH + 1, true, false),
        Err(DmaError::DescriptorLength {
            length: ADMA2_MAX_LENGTH + 1
        })
    );
    assert_eq!(
        Adma2Descriptor::transfer(0xc000_0000, 4095, true, false),
        Err(DmaError::MisalignedLength {
            length: 4095,
            alignment: 4
        })
    );
    assert_eq!(
        Adma2Descriptor::transfer(0xc000_0002, 4096, true, false),
        Err(DmaError::MisalignedAddress {
            address: 0xc000_0002,
            alignment: 4
        })
    );
    assert_eq!(
        Adma2Descriptor::link(0x8002, false),
        Err(DmaError::MisalignedAddress {
            address: 0x8002,
            alignment: 4
        })
    );
}

#[test]
fn reserved_attribute_bits_and_actions_are_rejected() {
    assert_eq!(
        Adma2Descriptor::decode32(&[0x40, 0, 0x00, 0x10, 0, 0x10, 0, 0xc0]),
        Err(DmaError::ReservedAttribute { attribute: 0x40 })
    );
    assert_eq!(
        Adma2Descriptor::decode32(&[0x10, 0, 0x00, 0x10, 0, 0x10, 0, 0xc0]),
        Err(DmaError::InvalidAction)
    );
    assert_eq!(
        Adma2Descriptor::decode32(&[0x21, 0]),
        Err(DmaError::BufferTooSmall {
            needed: ADMA2_DESCRIPTOR32_LEN,
            available: 2
        })
    );
    assert_eq!(
        Adma2Descriptor::decode32(&[0; ADMA2_DESCRIPTOR32_LEN]),
        Ok(Adma2Descriptor::EMPTY)
    );
}

#[test]
fn a_direct_buffer_maps_through_the_pi4_window() {
    assert_eq!(
        map_dma(PI4_WINDOW, 0, 0x1000),
        Ok(DmaMapping::Direct {
            bus_address: 0xc000_0000
        })
    );
    assert_eq!(
        map_dma(PI4_WINDOW, 0x3fff_f000, 0x1000),
        Ok(DmaMapping::Direct {
            bus_address: 0xffff_f000
        })
    );
    assert_eq!(
        map_dma(PI4_WINDOW, 0x4000_0000, 0x1000),
        Ok(DmaMapping::Bounce {
            reason: BounceReason::AboveWindow
        })
    );
    assert_eq!(
        map_dma(
            DmaWindow {
                bus_base: 0xc000_0000,
                cpu_base: 0x1000,
                size: 0x1000,
            },
            0x800,
            0x10
        ),
        Ok(DmaMapping::Bounce {
            reason: BounceReason::BelowWindow
        })
    );
}

#[test]
fn empty_and_overflowing_buffers_are_rejected() {
    assert_eq!(map_dma(PI4_WINDOW, 0, 0), Err(DmaError::EmptyBuffer));
    assert_eq!(
        map_dma(PI4_WINDOW, u64::MAX, 2),
        Err(DmaError::LengthOverflow)
    );
    assert_eq!(
        Adma2Table::from_buffer(PI4_WINDOW, 0, 0, DescriptorWidth::Bits32),
        Err(DmaError::EmptyBuffer)
    );
    assert_eq!(
        Adma2Table::from_buffer(PI4_WINDOW, 0, 0x1002, DescriptorWidth::Bits32),
        Err(DmaError::MisalignedLength {
            length: 0x1002,
            alignment: 4
        })
    );
}

#[test]
fn sdma_requires_a_direct_32_bit_aligned_address() {
    let plan = plan_sdma(PI4_WINDOW, 0x1000, 0x1000).expect("sdma");
    assert_eq!(plan.bus_address, 0xc000_1000);
    assert_eq!(plan.length, 0x1000);

    let plan = plan_sdma(PI4_WINDOW, 0x3fff_f000, 0x1000).expect("last byte");
    assert_eq!(plan.bus_address, 0xffff_f000);

    assert_eq!(
        plan_sdma(PI4_WINDOW, 0x4000_0000, 0x1000),
        Err(DmaError::BounceRequired {
            reason: BounceReason::AboveWindow
        })
    );
    assert_eq!(
        plan_sdma(PI4_WINDOW, 2, 0x10),
        Err(DmaError::MisalignedAddress {
            address: 0xc000_0002,
            alignment: 4
        })
    );
    assert_eq!(
        plan_sdma(
            DmaWindow {
                bus_base: 0x1_0000_0000,
                cpu_base: 0,
                size: 0x1000,
            },
            0x10,
            0x10
        ),
        Err(DmaError::BounceRequired {
            reason: BounceReason::BusAddressTooHigh
        })
    );
}

#[test]
fn an_adma2_table_splits_a_buffer_at_65536_bytes() {
    let table = Adma2Table::from_buffer(PI4_WINDOW, 0x1000, 512 * 1024, DescriptorWidth::Bits32)
        .expect("table");
    assert_eq!(table.len(), 8);
    assert_eq!(table.data_bytes(), 512 * 1024);
    assert_eq!(table.encoded_len(), 9 * ADMA2_DESCRIPTOR32_LEN);
    assert!(!table.descriptors()[0].end);
    assert!(table.descriptors()[7].end);
    assert_eq!(table.descriptors()[0].address, 0xc000_1000);
    assert_eq!(table.descriptors()[1].address, 0xc001_1000);
    assert_eq!(table.descriptors()[7].address, 0xc007_1000);
    assert_eq!(table.validate(PI4_WINDOW), Ok(()));

    let mut bytes = [0u8; 9 * ADMA2_DESCRIPTOR32_LEN];
    assert_eq!(table.encode(&mut bytes), Ok(bytes.len()));
    assert_eq!(
        Adma2Descriptor::decode32(&bytes[..8]),
        Ok(table.descriptors()[0])
    );
    assert_eq!(
        Adma2Descriptor::decode32(&bytes[56..64]),
        Ok(table.descriptors()[7])
    );
    assert_eq!(&bytes[64..72], &[0; 8]);
}

#[test]
fn a_64_bit_table_encodes_twelve_byte_descriptors() {
    let window = DmaWindow {
        bus_base: 0x1_0000_0000,
        cpu_base: 0,
        size: 0x4000_0000,
    };
    let table =
        Adma2Table::from_buffer(window, 0x1000, 0x2000, DescriptorWidth::Bits64).expect("table");
    assert_eq!(table.len(), 1);
    assert_eq!(table.encoded_len(), 2 * ADMA2_DESCRIPTOR64_LEN);
    let mut bytes = [0u8; 2 * ADMA2_DESCRIPTOR64_LEN];
    assert_eq!(table.encode(&mut bytes), Ok(bytes.len()));
    assert_eq!(
        Adma2Descriptor::decode64(&bytes[..12]),
        Ok(table.descriptors()[0])
    );
    assert_eq!(&bytes[12..24], &[0; 12]);

    assert_eq!(
        Adma2Table::from_buffer(window, 0x1000, 0x2000, DescriptorWidth::Bits32),
        Err(DmaError::BounceRequired {
            reason: BounceReason::BusAddressTooHigh
        })
    );
}

#[test]
fn a_crossing_or_outside_buffer_needs_a_bounce() {
    assert_eq!(
        Adma2Table::from_buffer(PI4_WINDOW, 0x3fff_f000, 0x2000, DescriptorWidth::Bits32),
        Err(DmaError::BounceRequired {
            reason: BounceReason::AboveWindow
        })
    );
    assert_eq!(
        Adma2Table::from_buffer(PI4_WINDOW, 0x4000_0000, 0x1000, DescriptorWidth::Bits32),
        Err(DmaError::BounceRequired {
            reason: BounceReason::AboveWindow
        })
    );
}

#[test]
fn a_table_beyond_sixteen_descriptors_is_full() {
    assert_eq!(
        Adma2Table::from_buffer(
            PI4_WINDOW,
            0,
            16 * ADMA2_MAX_LENGTH + 4096,
            DescriptorWidth::Bits32
        ),
        Err(DmaError::TableFull)
    );
}

#[test]
fn the_end_bit_must_appear_exactly_once_and_last() {
    let mut table = Adma2Table::new(DescriptorWidth::Bits32);
    table
        .push(Adma2Descriptor::transfer(0xc000_0000, 0x1000, false, false).expect("descriptor"))
        .expect("push");
    assert_eq!(table.validate(PI4_WINDOW), Err(DmaError::MissingEnd));

    let mut table = Adma2Table::new(DescriptorWidth::Bits32);
    table
        .push(Adma2Descriptor::transfer(0xc000_0000, 0x1000, true, false).expect("descriptor"))
        .expect("push");
    table
        .push(Adma2Descriptor::transfer(0xc000_1000, 0x1000, true, false).expect("descriptor"))
        .expect("push");
    assert_eq!(table.validate(PI4_WINDOW), Err(DmaError::EarlyEnd));
}

#[test]
fn descriptors_outside_the_bus_window_are_rejected() {
    let mut table = Adma2Table::new(DescriptorWidth::Bits32);
    table
        .push(Adma2Descriptor::transfer(0x1_0000_0000, 0x1000, true, false).expect("descriptor"))
        .expect("push");
    assert_eq!(
        table.validate(PI4_WINDOW),
        Err(DmaError::OutOfWindow {
            address: 0x1_0000_0000,
            length: 0x1000
        })
    );
}

#[test]
fn the_table_encoder_checks_the_buffer_and_terminator() {
    let table =
        Adma2Table::from_buffer(PI4_WINDOW, 0, 0x1000, DescriptorWidth::Bits32).expect("table");
    assert_eq!(
        table.encode(&mut [0u8; 15]),
        Err(DmaError::BufferTooSmall {
            needed: 16,
            available: 15
        })
    );
}

#[test]
fn plan_adma2_validates_the_table_placement() {
    let plan = plan_adma2(PI4_WINDOW, 0x1000, 0x1000, 0x8000, DescriptorWidth::Bits32)
        .expect("adma2 plan");
    assert_eq!(plan.data_bytes, 0x1000);
    assert_eq!(plan.table_bus_address, 0xc000_8000);
    assert_eq!(plan.table.len(), 1);
    assert_eq!(plan.table.encoded_len(), 16);

    assert_eq!(
        plan_adma2(PI4_WINDOW, 0x1000, 0x1000, 0x8002, DescriptorWidth::Bits32),
        Err(DmaError::MisalignedAddress {
            address: 0xc000_8002,
            alignment: 4
        })
    );
    assert_eq!(
        plan_adma2(
            PI4_WINDOW,
            0x1000,
            0x1000,
            0x4000_0000,
            DescriptorWidth::Bits32
        ),
        Err(DmaError::BounceRequired {
            reason: BounceReason::AboveWindow
        })
    );
}
