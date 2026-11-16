// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the transfer-ring device model.

use core::cell::{Cell, RefCell};

use super::*;
use crate::descriptor::BOOT_DESCRIPTOR;

#[derive(Default)]
struct Fake {
    controls: RefCell<[SetupPacket; 8]>,
    control_count: Cell<usize>,
    submitted: Cell<usize>,
    reports: RefCell<[[u8; BOOT_REPORT_LEN]; 4]>,
    report_count: Cell<usize>,
    report_index: Cell<usize>,
}

impl TransferRing for Fake {
    fn control_out(&mut self, setup: SetupPacket, _data: &[u8]) -> Result<(), Error> {
        let index = self.control_count.get();
        if let Some(slot) = self.controls.borrow_mut().get_mut(index) {
            *slot = setup;
        }
        self.control_count.set(index.saturating_add(1));
        Ok(())
    }

    fn control_in(&mut self, _setup: SetupPacket, _data: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }

    fn interrupt_in(&mut self, endpoint: u8, length: usize) -> Result<(), Error> {
        assert_eq!(endpoint, REPORT_ENDPOINT);
        assert_eq!(length, BOOT_REPORT_LEN);
        self.submitted.set(self.submitted.get().saturating_add(1));
        Ok(())
    }

    fn next_report(&mut self) -> Option<[u8; BOOT_REPORT_LEN]> {
        let index = self.report_index.get();
        if index >= self.report_count.get() {
            return None;
        }
        self.report_index.set(index.saturating_add(1));
        self.reports.borrow().get(index).copied()
    }
}

#[test]
fn init_requests_boot_protocol_and_primes_the_ring() {
    let mut device = BootKeyboard::new(Fake::default());
    let layout = device.init(&BOOT_DESCRIPTOR).expect("init");
    assert!(layout.is_boot_keyboard());
    assert_eq!(device.ring().control_count.get(), 2);
    assert_eq!(
        device.ring().controls.borrow()[0],
        SetupPacket::set_protocol(0)
    );
    assert_eq!(
        device.ring().controls.borrow()[1],
        SetupPacket::set_idle(0, 0)
    );
    assert_eq!(device.ring().submitted.get(), 1);
}

#[test]
fn a_report_produces_events_and_resubmits() {
    let fake = Fake::default();
    fake.reports.borrow_mut()[0] = [0x02, 0x00, 0x04, 0, 0, 0, 0, 0];
    fake.report_count.set(1);
    let mut device = BootKeyboard::new(fake);
    let _ = device.init(&BOOT_DESCRIPTOR).expect("init");
    let events = device.poll().expect("poll").expect("events");
    assert_eq!(events.count, 2);
    assert_eq!(events.events[0].usage, 0xe1);
    assert_eq!(events.events[1].ascii, Some(b'A'));
    assert_eq!(device.ring().submitted.get(), 2);
    assert_eq!(device.poll(), Ok(None));
}

#[test]
fn a_non_keyboard_descriptor_is_rejected() {
    let mouse = [0x05, 0x01, 0x09, 0x02, 0xa1, 0x01, 0xc0];
    let mut device = BootKeyboard::new(Fake::default());
    assert_eq!(device.init(&mouse), Err(Error::NotAKeyboard));
    assert_eq!(device.poll(), Err(Error::NotInitialized));
    assert_eq!(device.ring().submitted.get(), 0);
}

#[test]
fn leds_use_the_output_report() {
    let mut device = BootKeyboard::new(Fake::default());
    let _ = device.init(&BOOT_DESCRIPTOR).expect("init");
    assert_eq!(device.submit_leds(0x02), Ok(()));
    assert_eq!(device.ring().control_count.get(), 3);
    assert_eq!(
        device.ring().controls.borrow()[2],
        SetupPacket::set_report(2, 0, 1)
    );
}
