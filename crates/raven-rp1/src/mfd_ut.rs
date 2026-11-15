// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the RP1 MFD parser.

use super::*;
use raven_fdt_lite::Fdt;
use raven_pci::Rp1Window;

use crate::gpio::{Direction, GpioError};

const MFD: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-mfd.dtb");
const VALID: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-valid.dtb");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-minimal.dtb");
const BAD_PINS: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-mfd-bad-pins.dtb");
const BAD_CLOCK: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-mfd-bad-clock.dtb");

fn parse(blob: &[u8]) -> Result<Mfd, MfdError> {
    let fdt = Fdt::parse(blob).expect("fixture FDT");
    Mfd::from_fdt(&fdt)
}

fn name(text: &str) -> Name {
    Name::new(text).expect("name")
}

#[test]
fn parses_the_gpio_block_and_pin_rules() {
    let mfd = parse(MFD).expect("mfd");
    let gpio = mfd.gpio.expect("gpio");
    assert_eq!(gpio.bank_count, 3);
    assert_eq!(
        gpio.bank(0).map(|region| (region.base(), region.size())),
        Some((0x00c0_400d_0000, 0xc000))
    );
    assert_eq!(
        gpio.bank(2).map(|region| region.base()),
        Some(0x00c0_400f_0000)
    );
    assert_eq!(gpio.bank(3), None);
    assert_eq!(gpio.pins, 54);
    assert_eq!(gpio.interrupt_count, 3);
    assert_eq!(gpio.interrupts[0], Interrupt { id: 0, flags: 4 });
    assert_eq!(gpio.interrupts[2], Interrupt { id: 2, flags: 4 });

    let pinctrl = mfd.pinctrl();
    assert_eq!(pinctrl.len(), 6);
    assert!(pinctrl.allows(14, name("uart0")));
    assert!(pinctrl.allows(15, name("uart0")));
    assert!(pinctrl.allows(0, name("i2c0")));
    assert!(!pinctrl.allows(14, name("i2c0")));
    assert_eq!(pinctrl.bias_for(15, name("uart0")), Some(Bias::PullUp));
    assert_eq!(pinctrl.bias_for(14, name("uart0")), Some(Bias::Disable));
}

#[test]
fn parses_the_clock_provider_and_assigned_rates() {
    let mfd = parse(MFD).expect("mfd");
    let clocks = mfd.clocks.expect("clocks");
    assert_eq!(clocks.region.base(), 0x00c0_4001_8000);
    assert_eq!(clocks.region.size(), 0x1_0038);
    assert_eq!(clocks.tree.len(), 13);
    assert_eq!(
        clocks.tree.get(0).map(|clock| clock.rate_hz),
        Some(1_000_000_000)
    );
    assert_eq!(
        clocks.tree.get(1).map(|clock| clock.rate_hz),
        Some(1_536_000_000)
    );
    assert_eq!(
        clocks.tree.get(10).map(|clock| clock.rate_hz),
        Some(153_600_000)
    );
    assert_eq!(
        clocks.tree.get(31).map(|clock| clock.rate_hz),
        Some(1_000_000)
    );
    assert_eq!(
        clocks.tree.get(29).map(|clock| clock.rate_hz),
        Some(50_000_000)
    );
    assert_eq!(clocks.tree.get(99), None);
}

#[test]
fn parses_the_function_windows_and_firmware() {
    let mfd = parse(MFD).expect("mfd");
    assert_eq!(mfd.function_count(), 7);

    let ethernet = mfd.function(FunctionKind::Ethernet).expect("ethernet");
    assert_eq!(
        (ethernet.region.base(), ethernet.region.size()),
        (0x00c0_4010_0000, 0x4000)
    );
    assert!(ethernet.enabled);

    let usb = mfd.function(FunctionKind::Usb).expect("usb");
    assert_eq!(usb.region.base(), 0x00c0_4020_0000);
    assert!(!usb.enabled);

    let sram = mfd.function(FunctionKind::Sram).expect("sram");
    assert_eq!(sram.region.base(), 0x00c0_4040_0000);
    assert_eq!(sram.region.size(), 0x1_0000);

    let firmware = mfd.firmware.expect("firmware");
    assert_eq!(firmware.mailbox.base(), 0x00c0_4000_8000);
    assert_eq!(firmware.shmem.base(), 0x00c0_4040_ff00);
    assert_eq!(firmware.shmem.size(), 0x100);
}

#[test]
fn builds_the_pin_controller_from_the_tree() {
    let mfd = parse(MFD).expect("mfd");
    let mut controller = mfd.pin_controller().expect("controller");
    assert_eq!(controller.pin_count(), 54);
    let owner = name("uart");
    assert!(controller.claim(14, owner).is_ok());
    assert!(
        controller
            .configure(
                14,
                owner,
                Some(name("uart0")),
                Direction::Output,
                Bias::Disable
            )
            .is_ok()
    );
    assert_eq!(
        controller.configure(
            14,
            owner,
            Some(name("i2c0")),
            Direction::Output,
            Bias::Disable
        ),
        Err(GpioError::FunctionNotOnPin {
            pin: 14,
            function: name("i2c0")
        })
    );
}

#[test]
fn the_mfd_windows_translate_through_the_r1_rp1_window() {
    let mfd = parse(MFD).expect("mfd");
    let pi5 = raven_pi5::parse(MFD).expect("Pi 5 DTB");
    let pcie = pi5.pcie.expect("PCIe node");
    let rp1 = pcie.rp1.expect("RP1 node");
    let window =
        Rp1Window::new(rp1.child_base, rp1.pci_address, rp1.cpu_base, rp1.size).expect("window");

    let gpio = mfd.gpio.expect("gpio");
    let bank = gpio.bank(0).expect("bank 0");
    assert_eq!(window.translate(bank.base()), Some(0x1f_000d_0000));

    let ethernet = mfd.function(FunctionKind::Ethernet).expect("ethernet");
    assert_eq!(
        window.translate(ethernet.region.base()),
        Some(0x1f_0010_0000)
    );
}

#[test]
fn a_minimal_tree_has_only_the_provider() {
    let mfd = parse(VALID).expect("mfd");
    assert!(mfd.gpio.is_none());
    assert!(mfd.pin_controller().is_none());
    let clocks = mfd.clocks.expect("clocks");
    assert!(clocks.tree.is_empty());
    assert_eq!(mfd.function_count(), 3);
    assert!(mfd.firmware.is_none());
}

#[test]
fn a_tree_without_rp1_is_rejected() {
    let fdt = Fdt::parse(MINIMAL).expect("minimal FDT");
    assert_eq!(Mfd::from_fdt(&fdt), Err(MfdError::MissingRp1));
}

#[test]
fn malformed_mfd_data_is_rejected() {
    assert_eq!(parse(BAD_PINS), Err(MfdError::PinCount(0)));
    assert_eq!(
        parse(BAD_CLOCK),
        Err(MfdError::ClockCount {
            clocks: 13,
            rates: 12
        })
    );
}
