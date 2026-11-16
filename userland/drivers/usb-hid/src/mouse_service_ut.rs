// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the mouse service.

use core::cell::{Cell, RefCell};

use super::*;
use crate::pointer::PointerConsumer;
use crate::{BOOT_KEYBOARD_DESCRIPTOR, BOOT_MOUSE_DESCRIPTOR, SetupPacket};
use raven_usb_hid::Error;

#[derive(Default)]
struct Fake {
    reports: RefCell<[[u8; 8]; 8]>,
    count: Cell<usize>,
    index: Cell<usize>,
    controls: Cell<usize>,
    submits: Cell<usize>,
    lengths: RefCell<[usize; 8]>,
}

impl TransferRing for Fake {
    fn control_out(&mut self, _setup: SetupPacket, _data: &[u8]) -> Result<(), Error> {
        self.controls.set(self.controls.get().saturating_add(1));
        Ok(())
    }

    fn control_in(&mut self, _setup: SetupPacket, _data: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }

    fn interrupt_in(&mut self, _endpoint: u8, length: usize) -> Result<(), Error> {
        let index = self.submits.get();
        if let Some(slot) = self.lengths.borrow_mut().get_mut(index) {
            *slot = length;
        }
        self.submits.set(index.saturating_add(1));
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
        [0x01, 0x03, 0xfe, 0x00, 0, 0, 0, 0], // left press, dx 3, dy -2
        [0x00, 0x00, 0x00, 0x01, 0, 0, 0, 0], // left release, wheel 1
        [0x02, 0x01, 0x01, 0xff, 0, 0, 0, 0], // right press, dx 1, dy 1, wheel -1
    ];
    for (slot, report) in fake.reports.borrow_mut().iter_mut().zip(reports) {
        *slot = report;
    }
    fake.count.set(reports.len());
    fake
}

#[derive(Default)]
struct Recorder {
    events: [PointerEvent; 4],
    count: usize,
}

impl PointerConsumer for Recorder {
    fn pointer_event(&mut self, event: PointerEvent) {
        if let Some(slot) = self.events.get_mut(self.count) {
            *slot = event;
            self.count = self.count.saturating_add(1);
        }
    }
}

#[test]
fn init_selects_boot_protocol_and_primes() {
    let mut service = MouseService::new(Fake::default());
    assert_eq!(service.init(&BOOT_MOUSE_DESCRIPTOR), Ok(()));
    assert_eq!(service.ring().controls.get(), 2);
    assert_eq!(service.ring().submits.get(), 1);
    assert_eq!(service.ring().lengths.borrow()[0], 4);
    assert_eq!(service.mouse().report_len(), 4);
    assert_eq!(service.pointer_len(), 0);
}

#[test]
fn reports_become_pointer_events() {
    let mut service = MouseService::new(scripted());
    assert_eq!(service.init(&BOOT_MOUSE_DESCRIPTOR), Ok(()));
    for _ in 0..3 {
        assert_eq!(service.poll(), Ok(1));
    }
    assert_eq!(service.pointer_len(), 3);
    assert_eq!(service.ring().submits.get(), 4);
    assert_eq!(service.mouse().mouse().position(), (4, -1));

    let mut events = [PointerEvent::NONE; 4];
    assert_eq!(service.read_pointer(&mut events), 3);
    assert_eq!(events[0].pressed, raven_usb_hid::BUTTON_LEFT);
    assert_eq!((events[0].dx, events[0].dy, events[0].wheel), (3, -2, 0));
    assert_eq!(events[1].released, raven_usb_hid::BUTTON_LEFT);
    assert_eq!(events[1].wheel, 1);
    assert_eq!(events[2].pressed, raven_usb_hid::BUTTON_RIGHT);
    assert_eq!((events[2].dx, events[2].dy, events[2].wheel), (1, 1, -1));
    assert_eq!(service.poll(), Ok(0));
    assert_eq!(service.ring().submits.get(), 4);
}

#[test]
fn drain_pointer_reaches_the_consumer_seam() {
    let mut service = MouseService::new(scripted());
    assert_eq!(service.init(&BOOT_MOUSE_DESCRIPTOR), Ok(()));
    assert_eq!(service.poll(), Ok(1));
    assert_eq!(service.poll(), Ok(1));
    let mut recorder = Recorder::default();
    assert_eq!(service.drain_pointer(&mut recorder), 2);
    assert_eq!(recorder.count, 2);
    assert_eq!(recorder.events[0].pressed, raven_usb_hid::BUTTON_LEFT);
    assert_eq!(recorder.events[1].released, raven_usb_hid::BUTTON_LEFT);
    assert_eq!(service.pointer_len(), 0);
    assert_eq!(service.drain_pointer(&mut recorder), 0);
}

#[test]
fn poll_before_init_is_not_initialized() {
    let mut service = MouseService::new(Fake::default());
    assert_eq!(service.poll(), Err(MouseError::Hid(Error::NotInitialized)));
}

#[test]
fn a_keyboard_descriptor_is_rejected() {
    let mut service = MouseService::new(Fake::default());
    assert_eq!(
        service.init(&BOOT_KEYBOARD_DESCRIPTOR),
        Err(MouseError::NotAMouse)
    );
    assert_eq!(service.ring().submits.get(), 0);
    assert_eq!(service.poll(), Err(MouseError::Hid(Error::NotInitialized)));
}
