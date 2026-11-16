// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host test for the Pi 4 PWM audio device-tree fixture.

use raven_fdt_lite::Fdt;

use crate::registers::{CPRMAN_BASE, GPIO_BASE, PWM1_BASE, PWM1_SIZE};

const AUDIO_HEX: &str = include_str!("../../../tests/fixtures/pi4-audio/pi4-audio.dtb.hex");

fn decode_hex(text: &str) -> std::vec::Vec<u8> {
    let digits: std::vec::Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .map(|byte| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => panic!("fixture contains a non-hex byte"),
        })
        .collect();
    assert!(digits.len().is_multiple_of(2));
    digits
        .chunks(2)
        .map(|pair| (pair[0] << 4) | pair[1])
        .collect()
}

#[test]
fn the_fixture_names_pwm1_at_the_audio_base_and_size() {
    let bytes = decode_hex(AUDIO_HEX);
    let fdt = Fdt::parse(&bytes).expect("fixture parses");
    let pwm = fdt.find_node("/soc/pwm@7e20c800").expect("pwm node");
    assert!(pwm.compatible_with("brcm,bcm2835-pwm"));
    let (base, size) = pwm.reg_translated().next().expect("pwm reg");
    assert_eq!(base, PWM1_BASE);
    assert_eq!(size, PWM1_SIZE);
}

#[test]
fn the_fixture_has_the_gpio_audio_pin_group() {
    let bytes = decode_hex(AUDIO_HEX);
    let fdt = Fdt::parse(&bytes).expect("fixture parses");
    let gpio = fdt.find_node("/soc/gpio@7e200000").expect("gpio node");
    let (base, size) = gpio.reg_translated().next().expect("gpio reg");
    assert_eq!(base, GPIO_BASE);
    assert_eq!(size, 0xb4);

    let pins = fdt
        .find_node("/soc/gpio@7e200000/audio_pins")
        .expect("audio pins");
    let values: std::vec::Vec<u64> = pins.cells("brcm,pins", 4).collect();
    assert_eq!(values, [40, 41]);
    assert_eq!(pins.u32_property("brcm,function"), Some(4));
    assert_eq!(pins.u32_property("brcm,pull"), Some(0));
}

#[test]
fn the_fixture_names_the_cprman_window() {
    let bytes = decode_hex(AUDIO_HEX);
    let fdt = Fdt::parse(&bytes).expect("fixture parses");
    let cprman = fdt.find_node("/soc/cprman@7e101000").expect("cprman node");
    let (base, size) = cprman.reg_translated().next().expect("cprman reg");
    assert_eq!(base, CPRMAN_BASE);
    assert_eq!(size, 0x2000);
}
