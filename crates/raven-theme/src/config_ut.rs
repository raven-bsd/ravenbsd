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
fn a_valid_file_overrides_a_colour() {
    let file = ThemeFile::parse("[colors]\naccent = \"#ff00ff\"\n").expect("file");
    let palette = file.palette().expect("palette");
    assert_eq!(palette.get(Role::Accent), Rgb::new(0xff, 0, 0xff));
}

#[test]
fn a_builtin_name_selects_its_palette() {
    let file = ThemeFile::parse("[theme]\nname = \"mono\"\n").expect("file");
    let palette = file.palette().expect("palette");
    assert_eq!(palette.name, "mono");
    assert_eq!(palette.get(Role::Accent), Rgb::new(0xff, 0xff, 0xff));
}

#[test]
fn the_architecture_table_parses_and_maps_the_extra_roles() {
    let text = concat!(
        "[theme]\n",
        "name = \"raven-violet\"\n",
        "variant = \"dark\"\n",
        "\n",
        "[colors]\n",
        "accent = \"#b388ff\"\n",
        "accent_alt = \"#ff79c6\"\n",
        "info = \"#8be9fd\"\n",
        "success = \"#50fa7b\"\n",
        "warning = \"#ffb86c\"\n",
        "error = \"#ff5555\"\n",
        "muted = \"#6272a4\"\n",
        "foreground = \"#f8f8f2\"\n",
        "border = \"#44475a\"\n",
    );
    let palette = ThemeFile::parse(text)
        .expect("canonical table")
        .palette()
        .expect("palette");
    assert_eq!(palette.get(Role::Accent), Rgb::new(0xb3, 0x88, 0xff));
    assert_eq!(palette.get(Role::Keyword), Rgb::new(0xff, 0x79, 0xc6));
    assert_eq!(palette.get(Role::Path), Rgb::new(0xf8, 0xf8, 0xf2));
    assert_eq!(palette.get(Role::Number), Rgb::new(0xff, 0xb8, 0x6c));
    assert_eq!(palette.get(Role::String), Rgb::new(0x50, 0xfa, 0x7b));
}

#[test]
fn the_shipped_default_theme_parses() {
    let palette = ThemeFile::parse(include_str!("../../../etc/raven/theme.toml"))
        .expect("shipped theme")
        .palette()
        .expect("palette");
    assert_eq!(palette.name, "raven-violet");
    assert_eq!(palette.get(Role::Accent), Rgb::new(0xb3, 0x88, 0xff));
    assert_eq!(palette.get(Role::Keyword), Rgb::new(0xff, 0x79, 0xc6));
}

#[test]
fn an_explicit_role_wins_over_an_alias() {
    let file = ThemeFile::parse(
        "[colors]\naccent_alt = \"#ff0000\"\nkeyword = \"#00ff00\"\nforeground = \"#00ffff\"\npath = \"#ffff00\"\n",
    )
    .expect("file");
    let palette = file.palette().expect("palette");
    assert_eq!(palette.get(Role::Keyword), Rgb::new(0x00, 0xff, 0x00));
    assert_eq!(palette.get(Role::Path), Rgb::new(0xff, 0xff, 0x00));
}

#[test]
fn an_unknown_key_is_rejected() {
    assert!(ThemeFile::parse("nope = 1\n").is_err());
    assert!(ThemeFile::parse("[theme]\nnope = 1\n").is_err());
}

#[test]
fn an_invalid_hex_is_rejected() {
    assert!(ThemeFile::parse("[colors]\naccent = \"violet\"\n").is_err());
}

#[test]
fn a_low_contrast_colour_is_rejected() {
    assert!(
        ThemeFile::parse("[theme]\nbackground = \"#ffffff\"\n[colors]\nmuted = \"#eeeeee\"\n")
            .is_err()
    );
}

#[test]
fn a_low_contrast_border_is_allowed() {
    let file =
        ThemeFile::parse("[theme]\nbackground = \"#000000\"\n[colors]\nborder = \"#101010\"\n")
            .expect("border");
    assert!(file.palette().is_ok());
}

#[test]
fn a_light_variant_is_rejected() {
    assert!(ThemeFile::parse("[theme]\nvariant = \"light\"\n").is_err());
}

#[test]
fn an_unknown_colour_is_rejected() {
    assert!(ThemeFile::parse("[colors]\nno_such_role = \"#ffffff\"\n").is_err());
}

#[test]
fn the_user_file_wins_over_the_system_file() {
    let dir = scratch("precedence");
    let user = dir.join("user.toml");
    let system = dir.join("system.toml");
    std::fs::write(&user, "[theme]\nname = \"classic\"\n").expect("user file");
    std::fs::write(&system, "[theme]\nname = \"mono\"\n").expect("system file");
    let palette = ThemeFile::load_from(&user, &system).expect("palette");
    assert_eq!(palette.name, "classic");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_system_file_is_used_without_a_user_file() {
    let dir = scratch("system");
    let user = dir.join("user.toml");
    let system = dir.join("system.toml");
    std::fs::write(&system, "[theme]\nname = \"mono\"\n").expect("system file");
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
