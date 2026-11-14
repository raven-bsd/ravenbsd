// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_default_palette_is_raven_violet() {
    assert_eq!(palette::Palette::default().name, "raven-violet");
}

#[test]
fn the_role_names_are_stable() {
    assert_eq!(Role::Path.as_str(), "path");
    assert_eq!(Role::parse("keyword"), Some(Role::Keyword));
}
