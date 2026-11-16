// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S62 `usb-keyboard` fixture.
//!
//! Runs the USB HID driver service with a fake USB core: the core answers the boot-protocol
//! setup, hands over the standard boot descriptor and completes scripted boot reports. The
//! service decodes them into a console input queue; the fixture drains the queue and checks the
//! `hiB` + Enter sequence. No hardware and no real USB enumeration are involved; the S61 core and
//! the bound TTY are follow-ups.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use usb_hid::{
    BOOT_KEYBOARD_DESCRIPTOR, Error, KeyboardService, SetupPacket, TransferRing,
};

entry!(main);

const REPORTS: usize = 8;

/// A scripted USB core behind the transfer-ring seam.
struct FakeUsbCore {
    reports: [[u8; 8]; REPORTS],
    index: usize,
    controls: u32,
    submits: u32,
}

impl FakeUsbCore {
    const fn new(reports: [[u8; 8]; REPORTS]) -> Self {
        Self {
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
        Ok(())
    }

    fn control_in(&mut self, _setup: SetupPacket, _data: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }

    fn interrupt_in(&mut self, _endpoint: u8, _length: usize) -> Result<(), Error> {
        self.submits = self.submits.saturating_add(1);
        Ok(())
    }

    fn next_report(&mut self) -> Option<[u8; 8]> {
        let report = self.reports.get(self.index).copied();
        if report.is_some() {
            self.index = self.index.saturating_add(1);
        }
        report
    }
}

fn fail(message: &str) -> i32 {
    println!("usb-keyboard: fail {message}");
    1
}

fn main(_args: raven_rt::Args) -> i32 {
    let reports = [
        [0x00, 0, 0x0b, 0, 0, 0, 0, 0], // h
        [0x00, 0, 0x00, 0, 0, 0, 0, 0],
        [0x00, 0, 0x0c, 0, 0, 0, 0, 0], // i
        [0x00, 0, 0x00, 0, 0, 0, 0, 0],
        [0x02, 0, 0x05, 0, 0, 0, 0, 0], // shift + b
        [0x00, 0, 0x05, 0, 0, 0, 0, 0],
        [0x00, 0, 0x28, 0, 0, 0, 0, 0], // enter
        [0x00, 0, 0x00, 0, 0, 0, 0, 0],
    ];
    let mut service = KeyboardService::new(FakeUsbCore::new(reports));
    if service.init(&BOOT_KEYBOARD_DESCRIPTOR).is_err() {
        return fail("init");
    }
    if service.ring().controls != 2 {
        return fail("boot protocol requests");
    }
    println!("usb-keyboard: boot protocol ok");
    for _ in 0..REPORTS {
        match service.poll() {
            Ok(_) => {}
            Err(error) => return fail(error_text(error)),
        }
    }
    if service.console_len() != 4 {
        return fail("console length");
    }
    if service.ring().submits != 9 {
        return fail("resubmit count");
    }
    let mut console = [0u8; 8];
    let count = service.read_console(&mut console);
    if count != 4 || console.get(..4) != Some(&b"hiB\n"[..]) {
        return fail("console bytes");
    }
    if service.poll() != Ok(0) {
        return fail("extra report");
    }
    println!("usb-keyboard: keys=hiB");
    println!("usb-keyboard: ok");
    0
}

fn error_text(error: Error) -> &'static str {
    match error {
        Error::TooShort => "too-short",
        Error::BadItem => "bad-item",
        Error::NotAKeyboard => "not-a-keyboard",
        Error::ShortReport => "short-report",
        Error::NotInitialized => "not-initialized",
        Error::NoDevice => "no-device",
    }
}
