// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the GENET DMA bounce and maintenance rules.

use super::*;
use raven_fdt_lite::DmaWindow;

fn map() -> DmaMap {
    DmaMap::from_windows(&[
        DmaWindow {
            bus_base: 0x4_7c00_0000,
            cpu_base: 0xfc00_0000,
            size: 0x0380_0000,
        },
        DmaWindow {
            bus_base: 0,
            cpu_base: 0,
            size: 0x4_0000_0000,
        },
    ])
}

fn rules() -> DmaRules {
    DmaRules::new(map())
}

fn bounce(cpu_addr: u64, bus_addr: u64, len: usize) -> DmaMapping {
    DmaMapping {
        cpu_addr,
        bus_addr,
        len,
        bounce: false,
    }
}

#[test]
fn a_low_cpu_buffer_uses_the_identity_window() {
    let mapping = rules().route(0x1000, 2048, None).expect("mapping");
    assert_eq!(mapping.cpu_addr, 0x1000);
    assert_eq!(mapping.bus_addr, 0x1000);
    assert_eq!(mapping.len, 2048);
    assert!(!mapping.bounce);
}

#[test]
fn a_high_cpu_buffer_uses_the_alias_window() {
    let mapping = rules().route(0xfc00_1000, 2048, None).expect("mapping");
    assert_eq!(mapping.cpu_addr, 0xfc00_1000);
    assert_eq!(mapping.bus_addr, 0x4_7c00_1000);
    assert!(!mapping.bounce);
}

#[test]
fn a_buffer_that_crosses_a_window_edge_is_not_direct() {
    let mapping = rules().route(0x4_0000_0000 - 64, 2048, None);
    assert_eq!(mapping, Err(DmaError::BounceRequired));
}

#[test]
fn a_buffer_above_every_window_requires_a_bounce_buffer() {
    assert_eq!(
        rules().route(0x10_0000_0000, 2048, None),
        Err(DmaError::BounceRequired)
    );
}

#[test]
fn a_supplied_bounce_buffer_is_used_for_an_unmapped_range() {
    let mapping = rules()
        .route(
            0x10_0000_0000,
            2048,
            Some(bounce(0x20_0000, 0x20_0000, 2048)),
        )
        .expect("bounce mapping");
    assert_eq!(mapping.cpu_addr, 0x20_0000);
    assert_eq!(mapping.bus_addr, 0x20_0000);
    assert_eq!(mapping.len, 2048);
    assert!(mapping.bounce);
}

#[test]
fn a_short_bounce_buffer_is_rejected() {
    assert_eq!(
        rules().route(
            0x10_0000_0000,
            2048,
            Some(bounce(0x20_0000, 0x20_0000, 1024))
        ),
        Err(DmaError::BounceTooSmall {
            provided: 1024,
            needed: 2048,
        })
    );
}

#[test]
fn a_bounce_buffer_outside_the_windows_is_rejected() {
    assert_eq!(
        rules().route(
            0x10_0000_0000,
            2048,
            Some(bounce(0x10_0000_0000, 0x10_0000_0000, 2048))
        ),
        Err(DmaError::BounceNotDirect)
    );
}

#[test]
fn a_bounce_buffer_with_the_wrong_bus_address_is_rejected() {
    assert_eq!(
        rules().route(
            0x10_0000_0000,
            2048,
            Some(bounce(0x20_0000, 0x30_0000, 2048))
        ),
        Err(DmaError::BounceBusMismatch)
    );
}

#[test]
fn an_unaligned_bounce_buffer_is_rejected() {
    assert_eq!(
        rules().route(
            0x10_0000_0000,
            2048,
            Some(bounce(0x20_0008, 0x20_0008, 2048))
        ),
        Err(DmaError::BounceNotDirect)
    );
}

#[test]
fn a_bounce_mapping_cannot_be_nested() {
    let nested = DmaMapping {
        cpu_addr: 0x20_0000,
        bus_addr: 0x20_0000,
        len: 2048,
        bounce: true,
    };
    assert_eq!(
        rules().route(0x10_0000_0000, 2048, Some(nested)),
        Err(DmaError::BounceNotDirect)
    );
}

#[test]
fn zero_length_is_rejected() {
    assert_eq!(rules().route(0x1000, 0, None), Err(DmaError::ZeroLength));
}

#[test]
fn more_than_one_packet_buffer_is_rejected() {
    assert_eq!(
        rules().route(0x1000, MAX_DMA_BUFFER + 1, None),
        Err(DmaError::TooLong(MAX_DMA_BUFFER + 1))
    );
}

#[test]
fn a_zero_or_unaligned_cpu_address_is_rejected() {
    assert_eq!(rules().route(0, 64, None), Err(DmaError::Unaligned(0)));
    assert_eq!(
        rules().route(0x1001, 64, None),
        Err(DmaError::Unaligned(0x1001))
    );
}

#[test]
fn an_identity_map_never_needs_a_bounce() {
    let rules = DmaRules::new(DmaMap::identity());
    let mapping = rules
        .route(0x1234_5600, 2048, None)
        .expect("identity mapping");
    assert_eq!(mapping.bus_addr, 0x1234_5600);
    assert!(!mapping.bounce);
}

#[test]
fn a_non_coherent_device_cleans_before_tx_and_invalidates_after_rx() {
    let mapping = rules().route(0x1000, 2048, None).expect("mapping");
    let tx = mapping.maintenance(DmaDirection::ToDevice, false);
    assert!(tx.clean);
    assert!(!tx.invalidate);
    let rx = mapping.maintenance(DmaDirection::FromDevice, false);
    assert!(!rx.clean);
    assert!(rx.invalidate);
}

#[test]
fn a_coherent_device_needs_no_cache_maintenance() {
    let mapping = rules().route(0x1000, 2048, None).expect("mapping");
    assert_eq!(
        mapping.maintenance(DmaDirection::ToDevice, true),
        DmaMaintenance::NONE
    );
    assert_eq!(
        mapping.maintenance(DmaDirection::FromDevice, true),
        DmaMaintenance::NONE
    );
    assert!(DmaRules::with_coherence(map(), true).coherent());
    assert!(!rules().coherent());
}
