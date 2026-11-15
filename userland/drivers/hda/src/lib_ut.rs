// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the HDA byte-level core.

use super::*;

#[test]
fn commands_follow_the_4_8_12_8_layout() {
    assert_eq!(cmd12(0, 0x04, 0xF00, 0x05), 0x004F_0005);
    assert_eq!(cmd12(0, 0x02, 0x706, 0x10), 0x0027_0610);
    assert_eq!(cmd4(0, 0x02, 0x0002, 0x1234), 0x0022_1234);
}

#[test]
fn the_48k_format_word_is_stereo_s16le() {
    assert_eq!(format_48k(2), 0x11);
    assert_eq!(format_48k(1), 0x10);
    assert_eq!(format_48k(8), 0x17);
}

#[test]
fn the_bdl_entry_is_sixteen_bytes() {
    let mut buffer = [0u8; 16];
    let entry = BdlEntry {
        address: 0x1234_5678_9abc_def0,
        length: 0x200,
        ioc: true,
    };
    assert!(entry.write(&mut buffer, 0));
    assert_eq!(&buffer[0..8], &0x1234_5678_9abc_def0u64.to_le_bytes());
    assert_eq!(&buffer[8..12], &0x200u32.to_le_bytes());
    assert_eq!(buffer[12], 1);
    assert!(!entry.write(&mut buffer, 1));
}

#[test]
fn ring_geometry_matches_the_shared_header_rule() {
    let geometry = RingGeometry {
        period_frames: 480,
        period_count: 4,
        frame_bytes: 4,
    };
    assert_eq!(geometry.region_len(), 128 + 480 * 4 * 4);
    assert_eq!(geometry.cbl_bytes(), 480 * 4 * 4);
}

#[test]
fn the_target_ring_is_exactly_thirty_two_kib() {
    let geometry = ring_geometry_48k();
    assert_eq!(geometry.period_frames, 480);
    assert_eq!(geometry.period_count, 17);
    assert_eq!(geometry.frame_bytes, 4);
    assert_eq!(geometry.region_len(), 32 * 1024);
    assert_eq!(geometry.cbl_bytes(), 32_640);
}

#[test]
fn qemu_intel_hda_ids_and_class_are_fixed() {
    assert_eq!(QEMU_HDA_VENDOR, 0x8086);
    assert_eq!(QEMU_HDA_DEVICE, 0x2668);
    assert_eq!(PCI_CLASS_MULTIMEDIA, 0x04);
    assert_eq!(PCI_SUBCLASS_AUDIO, 0x03);
}

#[test]
fn widget_type_and_amplifier_bits_are_decoded() {
    assert_eq!(widget_type(0x0000_0000), WIDGET_AUDIO_OUTPUT);
    assert_eq!(widget_type(0x0040_0000), WIDGET_PIN_COMPLEX);
    assert!(widget_has_output_amp(0x0000_0004));
    assert!(!widget_has_output_amp(0x0000_0002));
    assert!(widget_has_input_amp(0x0000_0002));
    assert!(!widget_has_input_amp(0x0000_0004));
}

#[test]
fn pin_default_configuration_selects_outputs() {
    assert!(config_is_output(0x0221_401f)); // HP out
    assert!(config_is_output(0x0011_401f)); // line out
    assert!(config_is_output(0x0000_0000)); // line out, device 0
    assert!(!config_is_output(0x0091_401f)); // line in, device 9
}

#[test]
fn stream_control_places_tag_ioc_and_run() {
    assert_eq!(stream_control(1, false, false), 0x0010_0000);
    assert_eq!(stream_control(1, true, true), 0x0010_0006);
    assert_eq!(stream_control(0, true, false), 0x0000_0002);
}

#[test]
fn output_descriptors_follow_the_input_descriptors() {
    assert_eq!(stream_descriptor_offset(4, 0), 0x100);
    assert_eq!(stream_descriptor_offset(4, 1), 0x120);
    assert_eq!(stream_descriptor_offset(0, 0), SD_BASE);
}

#[test]
fn link_position_delta_handles_wrap() {
    assert_eq!(lpib_delta_frames(0, 1_920, 32_640, 4), 480);
    assert_eq!(lpib_delta_frames(31_680, 960, 32_640, 4), 480);
    assert_eq!(lpib_delta_frames(1_920, 1_920, 32_640, 4), 0);
    assert_eq!(lpib_delta_frames(0, 1_920, 32_640, 0), 0);
}

#[test]
fn register_offsets_are_the_ich9_values() {
    assert_eq!(GCTL, 0x08);
    assert_eq!(CORBLBASE, 0x40);
    assert_eq!(RIRBWP, 0x58);
    assert_eq!(ICOI, 0x60);
    assert_eq!(SD_BASE, 0x80);
    assert_eq!(SD_STRIDE, 0x20);
    assert_eq!(SD_BDPL, 0x18);
}
