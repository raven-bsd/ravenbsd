// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn raven_violet_is_the_default() {
    assert_eq!(Palette::default().name, "raven-violet");
}

#[test]
fn every_role_has_a_colour() {
    let palette = Palette::raven_violet();
    for role in Role::ALL {
        assert_ne!(palette.get(role), Rgb::default());
    }
}

#[test]
fn hex_round_trips() {
    let colour = Rgb::new(0x9d, 0x7c, 0xff);
    assert_eq!(Rgb::parse_hex(&colour.to_hex()), Some(colour));
}

#[test]
fn invalid_hex_is_refused() {
    assert!(Rgb::parse_hex("violet").is_none());
    assert!(Rgb::parse_hex("#12345").is_none());
}

#[test]
fn classic_and_mono_are_builtins() {
    assert!(Palette::builtin("classic").is_some());
    assert!(Palette::builtin("mono").is_some());
    assert!(Palette::builtin("nope").is_none());
}
