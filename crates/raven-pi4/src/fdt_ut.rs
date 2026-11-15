// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the EMMC2 device-tree lookup, using the committed fixtures.

use super::*;
use raven_fdt_lite::FdtError;

const VALID: &[u8] = include_bytes!("../../../tests/fixtures/pi4-emmc2/pi4-emmc2.dtb");
const NO_EMMC: &[u8] = include_bytes!("../../../tests/fixtures/pi4-emmc2/pi4-emmc2-no-emmc.dtb");
const BAD_IRQ: &[u8] = include_bytes!("../../../tests/fixtures/pi4-emmc2/pi4-emmc2-bad-irq.dtb");

#[test]
fn the_valid_fixture_yields_the_translated_base_spi_and_dma_window() {
    let node = parse_emmc2(VALID).expect("valid emmc2 tree");
    assert_eq!(node.base, 0xfe34_0000);
    assert_eq!(node.size, 0x100);
    assert_eq!(node.irq_spi, 126);
    assert_eq!(node.gic_interrupt_id(), 158);
    assert_eq!(
        node.dma,
        DmaWindow {
            bus_base: 0xc000_0000,
            cpu_base: 0,
            size: 0x4000_0000,
        }
    );
    assert_eq!(node.cpu_limit(), Some(0x4000_0000));
}

#[test]
fn dma_bus_addresses_are_limited_to_the_dtb_window() {
    let node = parse_emmc2(VALID).expect("valid emmc2 tree");
    assert_eq!(node.bus_address(0), Some(0xc000_0000));
    assert_eq!(node.bus_address(0x1000_0000), Some(0xd000_0000));
    assert_eq!(node.bus_address(0x3fff_f000), Some(0xffff_f000));
    assert_eq!(node.bus_address(0x4000_0000), None);
    assert_eq!(node.bus_address(u64::MAX), None);
}

#[test]
fn a_tree_without_the_emmc2_node_is_rejected() {
    assert_eq!(parse_emmc2(NO_EMMC), Err(Emmc2FdtError::MissingNode));
}

#[test]
fn an_active_low_interrupt_is_rejected() {
    assert_eq!(
        parse_emmc2(BAD_IRQ),
        Err(Emmc2FdtError::BadInterrupt {
            interrupt_type: 0,
            number: 126,
            flags: 0,
        })
    );
}

#[test]
fn a_blob_that_is_not_a_device_tree_is_rejected() {
    assert_eq!(
        parse_emmc2(b"not a device tree"),
        Err(Emmc2FdtError::Fdt(FdtError::TooShort))
    );
    assert_eq!(
        parse_emmc2(&[0u8; 40]),
        Err(Emmc2FdtError::Fdt(FdtError::BadMagic))
    );
}

#[test]
fn the_error_display_names_the_failed_fact() {
    assert_eq!(
        format!("{}", Emmc2FdtError::MissingNode),
        "device tree has no brcm,bcm2711-emmc2 node"
    );
    assert_eq!(
        format!(
            "{}",
            Emmc2FdtError::BadInterrupt {
                interrupt_type: 0,
                number: 126,
                flags: 0,
            }
        ),
        "emmc2 interrupt <0 126 0> is not a level-high SPI"
    );
}
