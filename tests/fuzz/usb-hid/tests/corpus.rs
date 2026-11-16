// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only corpus, truncation and mutation fuzz for the USB HID decoders.
//!
//! No `unsafe` and no external fuzz tooling: the deterministic [`Lcg`] makes every generated case
//! reproducible. A panic in any test fails the run, so "the tests pass" is the crash-free result.

mod seeds;

use raven_usb_hid::{
    BootKeyboard, BootReport, Error, Keyboard, MAX_EVENTS, SetupPacket, TransferRing, key_to_ascii,
    parse_hid_descriptor, parse_report_descriptor,
};
use raven_usb_hid_fuzz::Lcg;

const DESCRIPTOR_MUTATIONS: usize = 50_000;
const RANDOM_DESCRIPTORS: usize = 50_000;
const REPORT_ROUNDS: usize = 100_000;
const DEVICE_REPORTS: usize = 10_000;

#[test]
fn class_descriptor_seeds_are_handled() {
    for seed in seeds::HID_DESCRIPTORS {
        let _ = parse_hid_descriptor(seed);
    }
    let valid = [9u8, 0x21, 0x11, 0x01, 0, 1, 0x22, 63, 0];
    let hid = parse_hid_descriptor(&valid).expect("valid class descriptor");
    assert_eq!(hid.report_length, 63);
}

#[test]
fn report_descriptor_seeds_are_handled() {
    for seed in seeds::REPORT_DESCRIPTORS {
        let _ = parse_report_descriptor(seed);
    }
    let layout = parse_report_descriptor(&seeds::BOOT_DESCRIPTOR).expect("boot descriptor");
    assert!(layout.is_boot_keyboard());
    assert_eq!(layout.input_bits, 64);
}

#[test]
fn every_truncated_prefix_is_handled() {
    for length in 0..=seeds::BOOT_DESCRIPTOR.len() {
        let _ = parse_report_descriptor(&seeds::BOOT_DESCRIPTOR[..length]);
    }
    let class = [9u8, 0x21, 0x11, 0x01, 0, 1, 0x22, 63, 0];
    for length in 0..=class.len() {
        let _ = parse_hid_descriptor(&class[..length]);
    }
}

#[test]
fn descriptor_mutations_are_handled() {
    let mut generator = Lcg::new(0x5334);
    for _ in 0..DESCRIPTOR_MUTATIONS {
        let mut descriptor = seeds::BOOT_DESCRIPTOR;
        let flips = 1 + generator.below(4);
        for _ in 0..flips {
            let index = generator.below(descriptor.len());
            if let Some(slot) = descriptor.get_mut(index) {
                *slot ^= (generator.next_u32() & 0xff) as u8;
            }
        }
        let _ = parse_report_descriptor(&descriptor);
    }
}

#[test]
fn random_descriptors_are_handled() {
    let mut generator = Lcg::new(0x64_0001);
    let mut buffer = [0u8; 128];
    for _ in 0..RANDOM_DESCRIPTORS {
        let length = generator.below(buffer.len() + 1);
        if let Some(slice) = buffer.get_mut(..length) {
            generator.fill(slice);
            let _ = parse_report_descriptor(slice);
        }
        let class_length = generator.below(65);
        if let Some(slice) = buffer.get_mut(..class_length) {
            generator.fill(slice);
            let _ = parse_hid_descriptor(slice);
        }
    }
}

#[test]
fn oversized_counts_saturate() {
    let descriptor = [
        0x05u8, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x76, 0xff, 0xff, 0x96, 0xff, 0xff, 0x81, 0x02, 0xc0,
    ];
    let layout = parse_report_descriptor(&descriptor).expect("layout");
    assert!(layout.input_bits > 64);
    assert!(!layout.is_boot_keyboard());
}

#[test]
fn report_decoding_is_bounded() {
    let mut generator = Lcg::new(0x52_0001);
    let mut keyboard = Keyboard::new();
    for _ in 0..REPORT_ROUNDS {
        let mut bytes = [0u8; 8];
        generator.fill(&mut bytes);
        let report = BootReport::from_bytes(&bytes).expect("eight bytes");
        let events = keyboard.update(&report);
        assert!(events.count <= MAX_EVENTS);
        let short = generator.below(8);
        assert_eq!(
            BootReport::from_bytes(&bytes[..short]),
            Err(Error::ShortReport)
        );
    }
}

#[test]
fn keymap_covers_every_usage() {
    for usage in 0u16..=255 {
        let usage = usage as u8;
        for shift in [false, true] {
            for caps in [false, true] {
                let _ = key_to_ascii(usage, shift, caps);
            }
        }
    }
    for usage in 0xe0u8..=0xe7 {
        assert_eq!(key_to_ascii(usage, false, false), None);
    }
}

struct RandomRing {
    generator: Lcg,
    remaining: usize,
}

impl TransferRing for RandomRing {
    fn control_out(&mut self, _setup: SetupPacket, _data: &[u8]) -> Result<(), Error> {
        Ok(())
    }

    fn control_in(&mut self, _setup: SetupPacket, _data: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }

    fn interrupt_in(&mut self, _endpoint: u8, _length: usize) -> Result<(), Error> {
        Ok(())
    }

    fn next_report(&mut self) -> Option<[u8; 8]> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining = self.remaining.saturating_sub(1);
        let mut report = [0u8; 8];
        self.generator.fill(&mut report);
        Some(report)
    }
}

#[test]
fn device_model_survives_random_reports() {
    let ring = RandomRing {
        generator: Lcg::new(0x6400_0002),
        remaining: DEVICE_REPORTS,
    };
    let mut device = BootKeyboard::new(ring);
    device.init(&seeds::BOOT_DESCRIPTOR).expect("init");
    let mut polls = 0usize;
    while let Some(events) = device.poll().expect("poll") {
        assert!(events.count <= MAX_EVENTS);
        polls = polls.saturating_add(1);
        assert!(polls <= DEVICE_REPORTS);
    }
    assert_eq!(polls, DEVICE_REPORTS);
}

#[test]
fn rollover_reports_are_ignored() {
    let mut keyboard = Keyboard::new();
    let report =
        BootReport::from_bytes(&[0x02, 0, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01]).expect("rollover");
    let events = keyboard.update(&report);
    assert_eq!(events.count, 0);
}
