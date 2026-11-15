// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end assertions over the Pi 5 fixtures and the controller model.

use super::*;
use raven_fdt_lite::Fdt;
use raven_pci::Rp1Window;
use raven_rp1::{FunctionKind, Mfd};

const XHCI: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-xhci.dtb");

#[test]
fn the_usb_windows_match_the_mfd_function_windows() {
    let fdt = Fdt::parse(XHCI).expect("fixture FDT");
    let controllers = UsbControllers::from_fdt(&fdt).expect("USB nodes");
    let mfd = Mfd::from_fdt(&fdt).expect("MFD");

    let first = controllers.get(0).expect("controller 0");
    let function = mfd.function(FunctionKind::Usb).expect("USB function");
    assert_eq!(function.region, first.window);

    let second = controllers.get(1).expect("controller 1");
    let mut usb_windows = mfd
        .functions()
        .filter(|window| window.kind == FunctionKind::Usb);
    let first_mfd = usb_windows.next().expect("first USB");
    let second_mfd = usb_windows.next().expect("second USB");
    assert_eq!(first_mfd.region, first.window);
    assert_eq!(second_mfd.region, second.window);
}

#[test]
fn the_usb_windows_translate_through_the_r1_rp1_window() {
    let fdt = Fdt::parse(XHCI).expect("fixture FDT");
    let controllers = UsbControllers::from_fdt(&fdt).expect("USB nodes");
    let pi5 = raven_pi5::parse(XHCI).expect("Pi 5 DTB");
    let pcie = pi5.pcie.expect("PCIe node");
    let rp1 = pcie.rp1.expect("RP1 node");
    let window =
        Rp1Window::new(rp1.child_base, rp1.pci_address, rp1.cpu_base, rp1.size).expect("window");

    let first = controllers.get(0).expect("controller 0");
    let second = controllers.get(1).expect("controller 1");
    assert_eq!(window.translate(first.window.base()), Some(0x1f_0020_0000));
    assert_eq!(window.translate(second.window.base()), Some(0x1f_0030_0000));
}

#[test]
fn a_controller_plan_brings_up_a_port_and_a_slot() {
    let fdt = Fdt::parse(XHCI).expect("fixture FDT");
    let controllers = UsbControllers::from_fdt(&fdt).expect("USB nodes");
    let controller = controllers.get(0).expect("controller 0");

    let capability = Capability::decode(0x0110, 32 | (1 << 8) | (2 << 24), 1 | (1 << 3));
    assert_eq!(capability.max_slots, 32);
    assert_eq!(capability.max_ports, 2);
    assert!(capability.ac64);

    let mut hub = Roothub::new(capability.max_ports).expect("roothub");
    assert_eq!(hub.port_count(), 2);
    assert_eq!(hub.event(0, PortEvent::Connect), Ok(PortAction::Reset));
    assert_eq!(hub.event(0, PortEvent::LinkUp), Ok(PortAction::Enabled));
    assert_eq!(hub.enabled_count(), 1);

    let mut slots = SlotTable::new(capability.max_slots).expect("slots");
    let slot = slots.enable().expect("slot");
    assert_eq!(slots.address(slot), Ok(()));
    assert_eq!(slots.configure(slot), Ok(()));
    assert_eq!(slots.endpoint_enable(slot, 2), Ok(()));

    let mut commands = CommandRing::new(8).expect("command ring");
    assert_eq!(commands.push(Trb::enable_slot()), Ok(0));
    assert_eq!(
        commands.push(Trb::address_device(slot, 0x1000).expect("address")),
        Ok(1)
    );
    assert_eq!(commands.pending(), 2);

    let mut events = EventRing::new(8).expect("event ring");
    events
        .push_event(Trb::command_completion(slot, 1, 0x1000).expect("completion"))
        .expect("event");
    let mut irq = Interrupter::new();
    irq.write_iman(crate::regs::IMAN_IE);
    irq.raise();
    assert!(irq.active());
    let (_, event) = events.pop().expect("completion event");
    assert_eq!(event.completion_code(), 1);
    irq.write_erdp(0x2000);
    assert!(!irq.ehb());
    assert_eq!(controller.window.base(), 0x00c0_4020_0000);
}
