// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the HID descriptor parsers.

use super::*;

const CLASS_DESCRIPTOR: [u8; 9] = [9, 0x21, 0x11, 0x01, 0, 1, 0x22, 0x3f, 0];

#[test]
fn the_class_descriptor_reports_the_report_length() {
    let hid = parse_hid_descriptor(&CLASS_DESCRIPTOR).expect("hid");
    assert_eq!(hid.version, 0x0111);
    assert_eq!(hid.country, 0);
    assert_eq!(hid.report_length, 63);
}

#[test]
fn a_bad_class_descriptor_is_rejected() {
    assert_eq!(parse_hid_descriptor(&[5, 0x21]), Err(Error::TooShort));
    assert_eq!(
        parse_hid_descriptor(&[9, 0x22, 0x11, 0x01, 0, 1, 0x22, 63, 0]),
        Err(Error::BadItem)
    );
    assert_eq!(
        parse_hid_descriptor(&[9, 0x21, 0x11, 0x01, 0, 1, 0x23, 1, 0]),
        Err(Error::BadItem)
    );
}

#[test]
fn the_boot_keyboard_descriptor_is_recognised() {
    let layout = parse_report_descriptor(&BOOT_DESCRIPTOR).expect("layout");
    assert_eq!(layout.usage_page, 1);
    assert_eq!(layout.usage, 6);
    assert_eq!(layout.report_id, 0);
    assert_eq!(layout.input_bits, 64);
    assert_eq!(layout.output_bits, 8);
    assert_eq!(layout.count, 5);
    assert!(layout.is_boot_keyboard());
}

#[test]
fn the_input_fields_are_laid_out() {
    let layout = parse_report_descriptor(&BOOT_DESCRIPTOR).expect("layout");
    assert_eq!(layout.fields[0].bit_offset, 0);
    assert_eq!(layout.fields[0].bit_size, 1);
    assert_eq!(layout.fields[0].count, 8);
    assert_eq!(layout.fields[0].usage_min, 0xe0);
    assert_eq!(layout.fields[0].usage_max, 0xe7);
    assert_eq!(layout.fields[2].bit_offset, 16);
    assert_eq!(layout.fields[2].bit_size, 8);
    assert_eq!(layout.fields[2].count, 6);
    assert_eq!(layout.fields[2].usage_max, 0x65);
    assert!(layout.fields[3].output);
    assert_eq!(layout.fields[3].usage_page, 8);
    assert_eq!(layout.fields[3].bit_size, 1);
    assert_eq!(layout.fields[4].bit_size, 3);
}

#[test]
fn a_non_keyboard_layout_is_not_accepted() {
    let mouse = [0x05, 0x01, 0x09, 0x02, 0xa1, 0x01, 0xc0];
    let layout = parse_report_descriptor(&mouse).expect("layout");
    assert!(!layout.is_boot_keyboard());
}

#[test]
fn the_boot_mouse_descriptors_are_recognised() {
    let three = parse_report_descriptor(&BOOT_MOUSE_3_DESCRIPTOR).expect("layout");
    assert_eq!(three.usage_page, GENERIC_DESKTOP);
    assert_eq!(three.usage, MOUSE_USAGE);
    assert_eq!(three.input_bits, 24);
    assert!(three.is_boot_mouse());
    assert_eq!(three.boot_mouse_report_len(), Some(3));
    assert!(!three.is_boot_keyboard());

    let four = parse_report_descriptor(&BOOT_MOUSE_DESCRIPTOR).expect("layout");
    assert_eq!(four.input_bits, 32);
    assert!(four.is_boot_mouse());
    assert_eq!(four.boot_mouse_report_len(), Some(4));
}

#[test]
fn a_keyboard_is_not_a_boot_mouse() {
    let layout = parse_report_descriptor(&BOOT_DESCRIPTOR).expect("layout");
    assert!(!layout.is_boot_mouse());
    assert_eq!(layout.boot_mouse_report_len(), None);
}

#[test]
fn a_mouse_report_id_disqualifies_the_boot_layout() {
    let descriptor = [
        0x05u8, 0x01, 0x09, 0x02, 0xa1, 0x01, 0x85, 0x01, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x95,
        0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0x05, 0x01, 0x09, 0x30,
        0x09, 0x31, 0x95, 0x02, 0x75, 0x08, 0x81, 0x06, 0xc0,
    ];
    let layout = parse_report_descriptor(&descriptor).expect("layout");
    assert_eq!(layout.report_id, 1);
    assert!(!layout.is_boot_mouse());
}

#[test]
fn a_wider_mouse_input_report_is_not_a_boot_mouse() {
    let descriptor = [
        0x05u8, 0x01, 0x09, 0x02, 0xa1, 0x01, 0x76, 0xff, 0xff, 0x96, 0xff, 0xff, 0x81, 0x02, 0xc0,
    ];
    let layout = parse_report_descriptor(&descriptor).expect("layout");
    assert!(layout.input_bits > 32);
    assert!(!layout.is_boot_mouse());
}

#[test]
fn a_wider_input_report_is_not_a_boot_keyboard() {
    // S64 fuzz finding: a keyboard usage with a 65535-bit input payload was accepted as the
    // fixed eight-byte boot layout. The boot protocol report is exactly 64 bits.
    let descriptor = [
        0x05u8, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x76, 0xff, 0xff, 0x96, 0xff, 0xff, 0x81, 0x02, 0xc0,
    ];
    let layout = parse_report_descriptor(&descriptor).expect("layout");
    assert!(layout.input_bits > 64);
    assert!(!layout.is_boot_keyboard());
}

#[test]
fn a_report_id_disqualifies_the_boot_layout() {
    let descriptor = [
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x85, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x75,
        0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x06, 0x75, 0x08,
        0x81, 0x00, 0xc0,
    ];
    let layout = parse_report_descriptor(&descriptor).expect("layout");
    assert_eq!(layout.report_id, 1);
    assert!(!layout.is_boot_keyboard());
}

#[test]
fn a_long_item_is_skipped() {
    let descriptor = [
        0xfe, 0x02, 0x10, 0xaa, 0xbb, 0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0xc0,
    ];
    let layout = parse_report_descriptor(&descriptor).expect("layout");
    assert_eq!(layout.usage, 6);
}

#[test]
fn a_truncated_report_item_is_rejected() {
    assert_eq!(parse_report_descriptor(&[0x06, 0x01]), Err(Error::TooShort));
}

#[test]
fn a_global_stack_overflow_is_rejected() {
    let descriptor = [0x05, 0x01, 0xa4, 0xa4, 0xa4, 0xa4, 0xa4];
    assert_eq!(parse_report_descriptor(&descriptor), Err(Error::BadItem));
}
