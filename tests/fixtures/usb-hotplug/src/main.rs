// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S68 HID hotplug fixture.
//!
//! Runs the S62/S66 driver services over a fake USB core with a controllable presence bit. The
//! keyboard case holds Shift+A, detaches the device (stuck modifier and key must be released,
//! repeat must stop), re-enumerates while the device is absent (must fail without attaching),
//! then re-adds it and checks an unshifted `b` report. The mouse case holds the left button,
//! detaches (a release event must be queued), re-enumerates, and checks a right-button report on
//! the new device without a stale left-button transition. No hardware or real USB enumeration is
//! involved.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use usb_hid::{
    BOOT_KEYBOARD_DESCRIPTOR, BOOT_MOUSE_DESCRIPTOR, BUTTON_LEFT, BUTTON_RIGHT, Error,
    KeyboardService, MouseError, MouseService, PointerEvent, SetupPacket, TransferRing,
};

entry!(main);

const REPORTS: usize = 4;

/// A scripted USB core with a controllable presence bit.
struct FakeUsbCore {
    present: bool,
    reports: [[u8; 8]; REPORTS],
    index: usize,
    controls: u32,
    submits: u32,
}

impl FakeUsbCore {
    const fn new(reports: [[u8; 8]; REPORTS]) -> Self {
        Self {
            present: true,
            reports,
            index: 0,
            controls: 0,
            submits: 0,
        }
    }
}

impl TransferRing for FakeUsbCore {
    fn control_out(&mut self, _setup: SetupPacket, _data: &[u8]) -> Result<(), Error> {
        self.controls = self.controls.saturating_add(1);
        if self.present {
            Ok(())
        } else {
            Err(Error::NoDevice)
        }
    }

    fn control_in(&mut self, _setup: SetupPacket, _data: &mut [u8]) -> Result<usize, Error> {
        if self.present {
            Ok(0)
        } else {
            Err(Error::NoDevice)
        }
    }

    fn interrupt_in(&mut self, _endpoint: u8, _length: usize) -> Result<(), Error> {
        self.submits = self.submits.saturating_add(1);
        if self.present {
            Ok(())
        } else {
            Err(Error::NoDevice)
        }
    }

    fn next_report(&mut self) -> Option<[u8; 8]> {
        if !self.present {
            return None;
        }
        let report = self.reports.get(self.index).copied();
        if report.is_some() {
            self.index = self.index.saturating_add(1);
        }
        report
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(message) = keyboard_case() {
        println!("usb-hotplug: fail {message}");
        return 1;
    }
    if let Err(message) = mouse_case() {
        println!("usb-hotplug: fail {message}");
        return 1;
    }
    println!("usb-hotplug: ok");
    0
}

fn keyboard_case() -> Result<(), &'static str> {
    let reports = [
        [0x02, 0, 0x04, 0, 0, 0, 0, 0], // LShift + a
        [0x00, 0, 0x05, 0, 0, 0, 0, 0], // b on the re-added device
        [0; 8],
        [0; 8],
    ];
    let mut service = KeyboardService::new(FakeUsbCore::new(reports));
    service.init(&BOOT_KEYBOARD_DESCRIPTOR).map_err(hid_error)?;
    if service.ring().controls != 2 || service.ring().submits != 1 {
        return Err("keyboard attach");
    }
    println!("usb-hotplug: keyboard attach ok");

    if service.poll_at(0) != Ok(1) {
        return Err("keyboard press");
    }
    let mut console = [0u8; 4];
    if service.read_console(&mut console) != 1 || console[0] != b'A' {
        return Err("keyboard bytes");
    }
    if service.tick(500_000_000) != 1 || service.read_console(&mut console) != 1 {
        return Err("keyboard repeat");
    }
    if console[0] != b'A' {
        return Err("keyboard repeat bytes");
    }

    let releases = service.detach();
    if releases.count != 2 {
        return Err("stuck release count");
    }
    let mut shift = false;
    let mut key = false;
    for event in releases.iter() {
        if event.pressed {
            return Err("stuck release is a press");
        }
        shift |= event.usage == 0xe1;
        key |= event.usage == 0x04;
    }
    if !shift || !key {
        return Err("stuck release usages");
    }
    println!("usb-hotplug: stuck releases=2");

    if service.repeat().active() || service.tick(1_000_000_000) != 0 {
        return Err("repeat after detach");
    }
    if service.keyboard().keyboard().modifiers() != 0 {
        return Err("modifiers after detach");
    }
    if service.poll() != Err(Error::NotInitialized) {
        return Err("poll after detach");
    }
    println!("usb-hotplug: repeat stopped");

    service.ring_mut().present = false;
    if service.init(&BOOT_KEYBOARD_DESCRIPTOR) != Err(Error::NoDevice) {
        return Err("attach while absent");
    }
    if service.is_attached() {
        return Err("attached while absent");
    }

    service.ring_mut().present = true;
    service.ring_mut().index = 1;
    service.init(&BOOT_KEYBOARD_DESCRIPTOR).map_err(hid_error)?;
    if service.ring().controls != 5 || service.ring().submits != 3 {
        return Err("keyboard re-enumeration");
    }
    if !service.is_attached() || service.poll_at(10) != Ok(1) {
        return Err("keyboard reconnect press");
    }
    if service.read_console(&mut console) != 1 || console[0] != b'b' {
        return Err("keyboard reconnect bytes");
    }
    if service.keyboard().keyboard().shift() {
        return Err("stale shift");
    }
    println!("usb-hotplug: keyboard reconnect=b");
    Ok(())
}

fn mouse_case() -> Result<(), &'static str> {
    let reports = [
        [0x01, 0x02, 0x00, 0x00, 0, 0, 0, 0], // left press
        [0x02, 0x00, 0x00, 0x00, 0, 0, 0, 0], // right press on the re-added device
        [0; 8],
        [0; 8],
    ];
    let mut service = MouseService::new(FakeUsbCore::new(reports));
    service.init(&BOOT_MOUSE_DESCRIPTOR).map_err(mouse_error)?;
    if service.ring().controls != 2 || service.ring().submits != 1 {
        return Err("mouse attach");
    }
    if service.poll() != Ok(1) {
        return Err("mouse press");
    }
    let mut events = [PointerEvent::NONE; 2];
    if service.read_pointer(&mut events) != 1 || events[0].pressed != BUTTON_LEFT {
        return Err("mouse press event");
    }

    let release = service.detach().ok_or("mouse release missing")?;
    if release.released != BUTTON_LEFT || release.buttons != 0 || service.buttons() != 0 {
        return Err("mouse stuck release");
    }
    if service.pointer_len() != 1 || service.read_pointer(&mut events) != 1 {
        return Err("mouse release queue");
    }
    if events[0].released != BUTTON_LEFT {
        return Err("mouse release event");
    }
    if service.poll() != Err(MouseError::Hid(Error::NotInitialized)) {
        return Err("mouse poll after detach");
    }
    println!("usb-hotplug: mouse release ok");

    service.ring_mut().present = false;
    if service.init(&BOOT_MOUSE_DESCRIPTOR) != Err(MouseError::Hid(Error::NoDevice)) {
        return Err("mouse attach while absent");
    }
    if service.is_attached() {
        return Err("mouse attached while absent");
    }

    service.ring_mut().present = true;
    service.ring_mut().index = 1;
    service.init(&BOOT_MOUSE_DESCRIPTOR).map_err(mouse_error)?;
    if service.ring().controls != 5 || service.ring().submits != 3 {
        return Err("mouse re-enumeration");
    }
    if service.buttons() != 0 || service.poll() != Ok(1) {
        return Err("mouse reconnect press");
    }
    if service.read_pointer(&mut events) != 1
        || events[0].buttons != BUTTON_RIGHT
        || events[0].pressed != BUTTON_RIGHT
        || events[0].released != 0
    {
        return Err("mouse reconnect event");
    }
    println!("usb-hotplug: mouse reconnect=right");
    Ok(())
}

fn hid_error(error: Error) -> &'static str {
    match error {
        Error::TooShort => "keyboard descriptor too short",
        Error::BadItem => "keyboard descriptor item",
        Error::NotAKeyboard => "not a keyboard",
        Error::ShortReport => "short keyboard report",
        Error::NotInitialized => "keyboard not initialized",
        Error::NoDevice => "keyboard transport absent",
    }
}

fn mouse_error(error: MouseError) -> &'static str {
    match error {
        MouseError::Hid(error) => hid_error(error),
        MouseError::NotAMouse => "not a mouse",
    }
}
