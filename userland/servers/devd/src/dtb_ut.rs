// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the device-tree platform-bus parser.

use crate::dtb::*;

use raven_fdt_lite::{DmaWindow, FdtError};
use raven_proto_dev::parse_drivers;

const FIXTURE_HEX: &str = include_str!("../../../../tests/fixtures/pi4-devd/pi4-devd.dtb.hex");
const BAD_RANGES_HEX: &str =
    include_str!("../../../../tests/fixtures/pi4-devd/pi4-devd-bad-ranges.dtb.hex");

fn decode_hex(text: &str) -> Vec<u8> {
    let mut digits = String::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.is_empty() && !line.starts_with('#') {
            digits.push_str(line);
        }
    }
    digits
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn device<'a>(devices: &'a [DeviceSpec], path: &str) -> &'a DeviceSpec {
    devices
        .iter()
        .find(|device| device.path == path)
        .unwrap_or_else(|| panic!("missing device {path}"))
}

#[test]
fn fixture_lists_genet_emmc2_pwm1_and_rng200() {
    let fixture = decode_hex(FIXTURE_HEX);
    let devices = parse(&fixture).unwrap();
    assert_eq!(
        devices.len(),
        5,
        "four peripherals plus the GIC interrupt controller"
    );

    let genet = device(&devices, "/scb/ethernet@7d580000");
    assert_eq!(genet.compatible(), Some("brcm,bcm2711-genet-v5"));
    assert_eq!(
        genet.regs,
        [RegWindow {
            base: 0xfd58_0000,
            size: 0x1_0000
        }]
    );
    assert_eq!(genet.irq_label(), "spi=157,spi=158");
    assert!(genet.dma.is_empty(), "scb declares no dma-ranges");

    let emmc2 = device(&devices, "/emmc2bus/mmc@7e340000");
    assert_eq!(emmc2.compatible(), Some("brcm,bcm2711-emmc2"));
    assert_eq!(
        emmc2.regs,
        [RegWindow {
            base: 0xfe34_0000,
            size: 0x100
        }]
    );
    assert_eq!(emmc2.irq_label(), "spi=126");
    assert_eq!(
        emmc2.dma,
        [DmaWindow {
            bus_base: 0xc000_0000,
            cpu_base: 0x0,
            size: 0x4000_0000,
        }]
    );

    let pwm1 = device(&devices, "/soc/pwm@7e20c800");
    assert_eq!(pwm1.compatible(), Some("brcm,bcm2835-pwm"));
    assert_eq!(
        pwm1.regs,
        [RegWindow {
            base: 0xfe20_c800,
            size: 0x28
        }]
    );
    assert_eq!(pwm1.irq_label(), "none");

    let rng = device(&devices, "/soc/rng@7e104000");
    assert_eq!(rng.compatible(), Some("brcm,bcm2711-rng200"));
    assert_eq!(
        rng.regs,
        [RegWindow {
            base: 0xfe10_4000,
            size: 0x28
        }]
    );
    assert_eq!(rng.irq_label(), "none");

    let gic = device(&devices, "/interrupt-controller@ff841000");
    assert_eq!(gic.compatible(), Some("arm,gic-400"));
    assert_eq!(
        gic.regs,
        [RegWindow {
            base: 0xff84_1000,
            size: 0x1000
        }]
    );
}

#[test]
fn drivers_match_by_compatible_string() {
    let devices = parse(&decode_hex(FIXTURE_HEX)).unwrap();
    let drivers = parse_drivers(
        r#"
[[driver]]
name = "genet"
compatible = "brcm,bcm2711-genet-v5"
exec = "/sbin/genet"

[[driver]]
name = "emmc2"
compatible = "brcm,bcm2711-emmc2"
exec = "/sbin/emmc2"

[[driver]]
name = "pwm1"
compatible = "brcm,bcm2835-pwm"
exec = "/sbin/pwm"

[[driver]]
name = "rng200"
compatible = "brcm,bcm2711-rng200"
exec = "/sbin/rng200"

[[driver]]
name = "pci-only"
vendor = 0x1234
device = 0x11e8
exec = "/sbin/dummy"
"#,
    )
    .unwrap();

    let matched = match_drivers(&devices, &drivers);
    assert_eq!(matched.len(), 4, "the PCI entry is not a platform match");
    let names: Vec<&str> = matched
        .iter()
        .map(|(_, driver)| driver.name.as_str())
        .collect();
    assert_eq!(names, ["genet", "emmc2", "pwm1", "rng200"]);
    let paths: Vec<&str> = matched
        .iter()
        .map(|(device, _)| device.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "/scb/ethernet@7d580000",
            "/emmc2bus/mmc@7e340000",
            "/soc/pwm@7e20c800",
            "/soc/rng@7e104000",
        ]
    );
}

#[test]
fn reg_outside_the_parent_ranges_is_an_error() {
    assert_eq!(
        parse(&decode_hex(BAD_RANGES_HEX)),
        Err(DtbError::UnmappedReg)
    );
}

#[test]
fn truncated_blob_is_an_fdt_error() {
    let fixture = decode_hex(FIXTURE_HEX);
    assert_eq!(
        parse(&fixture[..20]).unwrap_err(),
        DtbError::Fdt(FdtError::TooShort)
    );
}
