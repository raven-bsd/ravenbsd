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
fn raven_violet_matches_the_architecture_table() {
    let palette = Palette::raven_violet();
    let expected = [
        (Role::Accent, Rgb::new(0xb3, 0x88, 0xff)),
        (Role::Muted, Rgb::new(0x62, 0x72, 0xa4)),
        (Role::Border, Rgb::new(0x44, 0x47, 0x5a)),
        (Role::Success, Rgb::new(0x50, 0xfa, 0x7b)),
        (Role::Warning, Rgb::new(0xff, 0xb8, 0x6c)),
        (Role::Error, Rgb::new(0xff, 0x55, 0x55)),
        (Role::Info, Rgb::new(0x8b, 0xe9, 0xfd)),
        (Role::Path, Rgb::new(0xf8, 0xf8, 0xf2)),
        (Role::Number, Rgb::new(0xff, 0xb8, 0x6c)),
        (Role::String, Rgb::new(0x50, 0xfa, 0x7b)),
        (Role::Keyword, Rgb::new(0xff, 0x79, 0xc6)),
    ];
    for (role, colour) in expected {
        assert_eq!(palette.get(role), colour, "{role:?}");
    }
}

#[test]
fn the_env_spec_is_the_role_order() {
    let palette = Palette::raven_violet();
    assert_eq!(
        palette.to_env_spec(),
        "#b388ff:#6272a4:#44475a:#50fa7b:#ffb86c:#ff5555:#8be9fd:#f8f8f2:#ffb86c:#50fa7b:#ff79c6"
    );
}

#[test]
fn the_env_spec_round_trips() {
    let palette = Palette::raven_violet();
    assert_eq!(
        Palette::from_env_spec(&palette.to_env_spec()),
        Some(palette)
    );
    assert_eq!(
        Palette::from_env_spec(
            "b388ff:6272a4:44475a:50fa7b:ffb86c:ff5555:8be9fd:f8f8f2:ffb86c:50fa7b:ff79c6"
        ),
        Some(Palette::raven_violet())
    );
}

#[test]
fn the_env_spec_rejects_bad_shapes() {
    let palette = Palette::raven_violet();
    let mut short = palette.to_env_spec();
    short.truncate(short.rfind(':').unwrap_or(0));
    assert_eq!(Palette::from_env_spec(&short), None);
    assert_eq!(Palette::from_env_spec(""), None);
    assert_eq!(
        Palette::from_env_spec(&format!("{}:", palette.to_env_spec())),
        None
    );
    let mut invalid = palette.to_env_spec();
    let index = invalid.find('#').unwrap_or(0);
    invalid.replace_range(index + 1..index + 2, "x");
    assert_eq!(Palette::from_env_spec(&invalid), None);
}

#[test]
fn hex_round_trips() {
    let colour = Rgb::new(0xb3, 0x88, 0xff);
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
