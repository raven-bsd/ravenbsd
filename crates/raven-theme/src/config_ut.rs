// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-theme-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("scratch directory");
    path
}

#[test]
fn a_valid_file_overrides_a_role() {
    let file = ThemeFile::parse("[roles]\naccent = \"#ff00ff\"\n").expect("file");
    let palette = file.palette().expect("palette");
    assert_eq!(palette.get(Role::Accent), Rgb::new(0xff, 0, 0xff));
}

#[test]
fn a_builtin_name_selects_its_palette() {
    let file = ThemeFile::parse("name = \"mono\"\n").expect("file");
    let palette = file.palette().expect("palette");
    assert_eq!(palette.name, "mono");
    assert_eq!(palette.get(Role::Accent), Rgb::new(0xff, 0xff, 0xff));
}

#[test]
fn an_unknown_key_is_rejected() {
    assert!(ThemeFile::parse("nope = 1\n").is_err());
}

#[test]
fn an_invalid_hex_is_rejected() {
    assert!(ThemeFile::parse("[roles]\naccent = \"violet\"\n").is_err());
}

#[test]
fn a_low_contrast_role_is_rejected() {
    assert!(ThemeFile::parse("background = \"#ffffff\"\n[roles]\nmuted = \"#eeeeee\"\n").is_err());
}

#[test]
fn an_unknown_role_is_rejected() {
    assert!(ThemeFile::parse("[roles]\nno_such_role = \"#ffffff\"\n").is_err());
}

#[test]
fn the_user_file_wins_over_the_system_file() {
    let dir = scratch("precedence");
    let user = dir.join("user.toml");
    let system = dir.join("system.toml");
    std::fs::write(&user, "name = \"classic\"\n").expect("user file");
    std::fs::write(&system, "name = \"mono\"\n").expect("system file");
    let palette = ThemeFile::load_from(&user, &system).expect("palette");
    assert_eq!(palette.name, "classic");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_system_file_is_used_without_a_user_file() {
    let dir = scratch("system");
    let user = dir.join("user.toml");
    let system = dir.join("system.toml");
    std::fs::write(&system, "name = \"mono\"\n").expect("system file");
    let palette = ThemeFile::load_from(&user, &system).expect("palette");
    assert_eq!(palette.name, "mono");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_file_falls_back_to_raven_violet() {
    let dir = scratch("missing");
    let palette =
        ThemeFile::load_from(&dir.join("user.toml"), &dir.join("system.toml")).expect("palette");
    assert_eq!(palette.name, "raven-violet");
    let _ = std::fs::remove_dir_all(&dir);
}
