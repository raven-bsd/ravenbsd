// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the boot mouse decoder and device model.

use core::cell::{Cell, RefCell};

use super::*;
use crate::descriptor::{BOOT_DESCRIPTOR, BOOT_MOUSE_3_DESCRIPTOR, BOOT_MOUSE_DESCRIPTOR};
use crate::report::BOOT_REPORT_LEN;

#[derive(Default)]
struct Fake {
    reports: RefCell<[[u8; BOOT_REPORT_LEN]; 4]>,
    report_count: Cell<usize>,
    report_index: Cell<usize>,
    controls: RefCell<[SetupPacket; 4]>,
    control_count: Cell<usize>,
    submits: Cell<usize>,
    lengths: RefCell<[usize; 4]>,
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
        let index = self.submits.get();
        if let Some(slot) = self.lengths.borrow_mut().get_mut(index) {
            *slot = length;
        }
        self.submits.set(index.saturating_add(1));
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
fn from_bytes_decodes_buttons_and_signed_movement() {
    let report = MouseReport::from_bytes(&[0x03, 0x05, 0xfb, 0xff]).expect("report");
    assert_eq!(report.buttons, BUTTON_LEFT | BUTTON_RIGHT);
    assert!(report.button(BUTTON_LEFT));
    assert!(report.button(BUTTON_RIGHT));
    assert!(!report.button(BUTTON_MIDDLE));
    assert_eq!(report.dx, 5);
    assert_eq!(report.dy, -5);
    assert_eq!(report.wheel, -1);
}

#[test]
fn a_three_byte_report_has_no_wheel_and_clears_reserved_bits() {
    let report = MouseReport::from_bytes(&[0xf9, 0x80, 0x7f]).expect("report");
    assert_eq!(report.buttons, BUTTON_LEFT | BUTTON_BACK | BUTTON_FORWARD);
    assert_eq!(report.dx, -128);
    assert_eq!(report.dy, 127);
    assert_eq!(report.wheel, 0);
    assert_eq!(MouseReport::from_bytes(&[0, 0]), Err(Error::ShortReport));
}

#[test]
fn update_reports_button_transitions_and_movement() {
    let mut mouse = Mouse::new();
    let press = MouseReport {
        buttons: BUTTON_LEFT,
        dx: 3,
        dy: -2,
        wheel: 0,
    };
    let event = mouse.update(&press);
    assert_eq!(event.buttons, BUTTON_LEFT);
    assert_eq!(event.pressed, BUTTON_LEFT);
    assert_eq!(event.released, 0);
    assert_eq!((event.dx, event.dy, event.wheel), (3, -2, 0));
    assert!(!event.is_empty());
    assert_eq!(mouse.buttons(), BUTTON_LEFT);
    assert_eq!(mouse.position(), (3, -2));

    let release = MouseReport {
        buttons: 0,
        dx: 0,
        dy: 0,
        wheel: 1,
    };
    let event = mouse.update(&release);
    assert_eq!(event.pressed, 0);
    assert_eq!(event.released, BUTTON_LEFT);
    assert_eq!(event.wheel, 1);
    assert_eq!(mouse.buttons(), 0);
    assert_eq!(mouse.position(), (3, -2));
    assert_eq!(mouse.wheel(), 1);
}

#[test]
fn an_unchanged_report_is_empty() {
    let mut mouse = Mouse::new();
    let event = mouse.update(&MouseReport::default());
    assert!(event.is_empty());
    assert_eq!(mouse.position(), (0, 0));
}

#[test]
fn init_uses_the_descriptor_report_length() {
    let fake = Fake::default();
    let mut device = BootMouse::new(fake);
    let layout = device.init(&BOOT_MOUSE_DESCRIPTOR).expect("init");
    assert!(layout.is_boot_mouse());
    assert_eq!(device.report_len(), MOUSE_REPORT_LEN);
    assert_eq!(device.ring().control_count.get(), 2);
    assert_eq!(
        device.ring().controls.borrow()[0],
        SetupPacket::set_protocol(0)
    );
    assert_eq!(
        device.ring().controls.borrow()[1],
        SetupPacket::set_idle(0, 0)
    );
    assert_eq!(device.ring().submits.get(), 1);
    assert_eq!(device.ring().lengths.borrow()[0], 4);

    let fake = Fake::default();
    let mut device = BootMouse::new(fake);
    let _ = device.init(&BOOT_MOUSE_3_DESCRIPTOR).expect("init");
    assert_eq!(device.report_len(), 3);
    assert_eq!(device.ring().lengths.borrow()[0], 3);
}

#[test]
fn a_report_produces_an_event_and_resubmits() {
    let fake = Fake::default();
    fake.reports.borrow_mut()[0] = [0x01, 0x04, 0xfe, 0x02, 0, 0, 0, 0];
    fake.report_count.set(1);
    let mut device = BootMouse::new(fake);
    let _ = device.init(&BOOT_MOUSE_DESCRIPTOR).expect("init");
    let event = device.poll().expect("poll").expect("event");
    assert_eq!(event.pressed, BUTTON_LEFT);
    assert_eq!((event.dx, event.dy, event.wheel), (4, -2, 2));
    assert_eq!(device.mouse().buttons(), BUTTON_LEFT);
    assert_eq!(device.ring().submits.get(), 2);
    assert_eq!(device.ring().lengths.borrow()[1], 4);
    assert_eq!(device.poll(), Ok(None));
}

#[test]
fn a_three_byte_report_is_polled_without_the_wheel_byte() {
    let fake = Fake::default();
    fake.reports.borrow_mut()[0] = [0x02, 0x01, 0x01, 0xff, 0, 0, 0, 0];
    fake.report_count.set(1);
    let mut device = BootMouse::new(fake);
    let _ = device.init(&BOOT_MOUSE_3_DESCRIPTOR).expect("init");
    let event = device.poll().expect("poll").expect("event");
    assert_eq!(event.buttons, BUTTON_RIGHT);
    assert_eq!((event.dx, event.dy, event.wheel), (1, 1, 0));
}

#[test]
fn a_non_mouse_descriptor_is_rejected() {
    let mut device = BootMouse::new(Fake::default());
    assert_eq!(device.init(&BOOT_DESCRIPTOR), Err(MouseError::NotAMouse));
    assert_eq!(device.poll(), Err(Error::NotInitialized.into()));
    assert_eq!(device.ring().submits.get(), 0);
}

#[test]
fn poll_before_init_is_not_initialized() {
    let mut device = BootMouse::new(Fake::default());
    assert_eq!(device.poll(), Err(MouseError::Hid(Error::NotInitialized)));
}
