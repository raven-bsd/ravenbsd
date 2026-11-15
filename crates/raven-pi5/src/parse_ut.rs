// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the Pi 5 device-tree parser.

use super::*;
use raven_fdt_lite::FdtError;

const VALID: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-valid.dtb");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-minimal.dtb");
const RP1_UART: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-uart-rp1.dtb");
const MEMORY_OVERLAP: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-memory-overlap.dtb");
const MEMORY_BAD_CELLS: &[u8] =
    include_bytes!("../../../tests/fixtures/pi5/pi5-memory-bad-cells.dtb");
const GIC_MISSING: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-gic-missing.dtb");
const BAD_MAGIC: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-bad-magic.dtb");

#[test]
fn parses_the_valid_pi5_platform() {
    let pi5 = parse(VALID).expect("valid Pi 5 DTB");
    assert!(pi5.is_raspberry_pi5());
    assert_eq!(pi5.model, "Raspberry Pi 5");
    assert_eq!(pi5.compatible, ["raspberrypi,5-model-b", "brcm,bcm2712"]);
    assert_eq!(pi5.chosen.stdout_path.as_deref(), Some("serial10:115200n8"));
    assert_eq!(
        pi5.chosen.bootargs.as_deref(),
        Some("console=ttyAMA0 loglevel=info")
    );

    assert_eq!(
        pi5.memory.regions,
        [MemoryRegion {
            base: 0,
            size: 0x2800_0000,
        }]
    );
    assert_eq!(
        pi5.memory.reserved,
        [MemoryRegion {
            base: 0,
            size: 0x8_0000,
        }]
    );

    assert_eq!(pi5.uart.node, "/soc@107c000000/serial@7d001000");
    assert_eq!(pi5.uart.base, 0x10_7d00_1000);
    assert_eq!(pi5.uart.size, 0x200);
    assert_eq!(pi5.uart.source, UartSource::Soc);
    assert_eq!(pi5.uart.interrupt, Some(0x79));
    assert_eq!(pi5.uart.clock_hz, None);

    assert_eq!(pi5.gic.distributor.base, 0x10_7fff_9000);
    assert_eq!(pi5.gic.distributor.size, 0x1000);
    assert_eq!(pi5.gic.cpu_interface.base, 0x10_7fff_a000);
    assert_eq!(pi5.gic.cpu_interface.size, 0x2000);
    assert_eq!(
        pi5.gic.virtual_interface.map(|region| region.base),
        Some(0x10_7fff_c000)
    );
    assert_eq!(
        pi5.gic.virtual_cpu.map(|region| region.base),
        Some(0x10_7fff_e000)
    );

    assert_eq!(pi5.timer.generic_ppi, 14);
    assert_eq!(pi5.timer.kernel_ppi, 30);
    let system = pi5.timer.system.expect("system timer");
    assert_eq!(system.base, 0x10_7c00_3000);
    assert_eq!(system.size, 0x1000);
    assert_eq!(system.clock_hz, 1_000_000);

    let pcie = pi5.pcie.expect("PCIe root complex");
    assert_eq!(pcie.node, "/axi/pcie@1000120000");
    assert_eq!(pcie.controller.base, 0x10_0012_0000);
    assert_eq!(pcie.controller.size, 0x9310);
    assert_eq!(pcie.ranges.len(), 2);
    let rp1 = pcie.rp1.expect("RP1");
    assert_eq!(rp1.child_base, 0xc0_4000_0000);
    assert_eq!(rp1.pci_address, 0);
    assert_eq!(rp1.cpu_base, 0x1f_0000_0000);
    assert_eq!(rp1.size, 0x41_0000);
    assert_eq!(rp1.translate(0xc0_4003_0000), Some(0x1f_0003_0000));
    assert_eq!(rp1.translate(0xc0_4041_0000), None);
}

#[test]
fn parses_a_minimal_tree_without_rp1() {
    let pi5 = parse(MINIMAL).expect("minimal Pi 5 DTB");
    assert_eq!(pi5.uart.base, 0x10_7d00_1000);
    assert!(pi5.pcie.is_none());
    assert!(pi5.memory.reserved.is_empty());
}

#[test]
fn selects_the_rp1_uart_through_the_pcie_window() {
    let pi5 = parse(RP1_UART).expect("RP1 UART DTB");
    assert_eq!(pi5.uart.node, "/axi/pcie@1000120000/rp1/serial@30000");
    assert_eq!(pi5.uart.source, UartSource::Rp1);
    assert_eq!(pi5.uart.base, 0x1f_0003_0000);
    assert_eq!(pi5.uart.size, 0x100);
}

#[test]
fn rejects_overlapping_memory_regions() {
    let error = parse(MEMORY_OVERLAP).expect_err("overlap must fail");
    assert!(
        matches!(error, Error::OverlappingMemory { .. }),
        "unexpected error: {error:?}"
    );
}

#[test]
fn rejects_malformed_memory_cells() {
    let error = parse(MEMORY_BAD_CELLS).expect_err("bad cells must fail");
    assert!(
        matches!(error, Error::BadMemory(_)),
        "unexpected error: {error:?}"
    );
}

#[test]
fn rejects_a_tree_without_a_gic() {
    let error = parse(GIC_MISSING).expect_err("missing GIC must fail");
    assert_eq!(error, Error::MissingGic);
}

#[test]
fn rejects_a_blob_with_a_bad_magic() {
    let error = parse(BAD_MAGIC).expect_err("bad magic must fail");
    assert_eq!(error, Error::Fdt(FdtError::BadMagic));
}
