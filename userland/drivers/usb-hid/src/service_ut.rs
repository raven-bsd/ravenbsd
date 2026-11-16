// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the keyboard service.

use core::cell::{Cell, RefCell};

use super::*;
use crate::BOOT_KEYBOARD_DESCRIPTOR;
use crate::SetupPacket;

struct Fake {
    reports: RefCell<[[u8; 8]; 8]>,
    count: Cell<usize>,
    index: Cell<usize>,
    controls: Cell<usize>,
    submits: Cell<usize>,
}

impl Default for Fake {
    fn default() -> Self {
        Self {
            reports: RefCell::new([[0u8; 8]; 8]),
            count: Cell::new(0),
            index: Cell::new(0),
            controls: Cell::new(0),
            submits: Cell::new(0),
        }
    }
}

impl TransferRing for Fake {
    fn control_out(&mut self, _setup: SetupPacket, _data: &[u8]) -> Result<(), Error> {
        self.controls.set(self.controls.get().saturating_add(1));
        Ok(())
    }

    fn control_in(&mut self, _setup: SetupPacket, _data: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }

    fn interrupt_in(&mut self, _endpoint: u8, _length: usize) -> Result<(), Error> {
        self.submits.set(self.submits.get().saturating_add(1));
        Ok(())
    }

    fn next_report(&mut self) -> Option<[u8; 8]> {
        let index = self.index.get();
        if index >= self.count.get() {
            return None;
        }
        self.index.set(index.saturating_add(1));
        self.reports.borrow().get(index).copied()
    }
}

fn scripted() -> Fake {
    let fake = Fake::default();
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
    for (slot, report) in fake.reports.borrow_mut().iter_mut().zip(reports) {
        *slot = report;
    }
    fake.count.set(reports.len());
    fake
}

#[test]
fn init_selects_boot_protocol_and_primes() {
    let mut service = KeyboardService::new(Fake::default());
    assert_eq!(service.init(&BOOT_KEYBOARD_DESCRIPTOR), Ok(()));
    assert_eq!(service.ring().controls.get(), 2);
    assert_eq!(service.ring().submits.get(), 1);
    assert_eq!(service.console_len(), 0);
}

#[test]
fn reports_become_console_bytes() {
    let mut service = KeyboardService::new(scripted());
    assert_eq!(service.init(&BOOT_KEYBOARD_DESCRIPTOR), Ok(()));
    for _ in 0..8 {
        let _ = service.poll().expect("poll");
    }
    assert_eq!(service.console_len(), 4);
    let mut out = [0u8; 8];
    assert_eq!(service.read_console(&mut out), 4);
    assert_eq!(&out[..4], b"hiB\n");
    assert_eq!(service.poll(), Ok(0));
    assert_eq!(service.ring().submits.get(), 9);
}

#[test]
fn poll_before_init_is_not_initialized() {
    let mut service = KeyboardService::new(Fake::default());
    assert_eq!(service.poll(), Err(Error::NotInitialized));
}

#[test]
fn a_non_keyboard_descriptor_is_rejected() {
    let mouse = [0x05, 0x01, 0x09, 0x02, 0xa1, 0x01, 0xc0];
    let mut service = KeyboardService::new(Fake::default());
    assert_eq!(service.init(&mouse), Err(Error::NotAKeyboard));
    assert_eq!(service.ring().controls.get(), 2);
    assert_eq!(service.ring().submits.get(), 0);
}

#[test]
fn uk_layout_and_repeat_reach_the_queue() {
    use crate::repeat::RepeatSettings;

    let fake = Fake::default();
    fake.reports.borrow_mut()[0] = [0x02, 0, 0x20, 0, 0, 0, 0, 0]; // UK Shift+3 -> £
    fake.count.set(1);
    let mut service = KeyboardService::with_layout(fake, Layout::Uk);
    assert_eq!(service.init(&BOOT_KEYBOARD_DESCRIPTOR), Ok(()));
    service.set_repeat(RepeatSettings {
        delay_ns: 100,
        period_ns: 50,
    });
    assert_eq!(service.poll_at(1_000), Ok(1));
    let mut out = [0u8; 8];
    assert_eq!(service.read_console(&mut out), 1);
    assert_eq!(out[0], 0xa3);
    assert_eq!(service.tick(1_099), 0);
    assert_eq!(service.tick(1_100), 1);
    assert_eq!(service.read_console(&mut out), 1);
    assert_eq!(out[0], 0xa3);
}
