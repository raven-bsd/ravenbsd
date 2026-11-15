// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use raven_theme::Role;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-cli-theme-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("scratch directory");
    path
}

#[test]
fn the_meta_names_the_theme_command() {
    assert_eq!(THEME_COMMAND.meta().name, "theme");
    assert_eq!(
        THEME_COMMAND.meta().usage,
        "raven theme <preview|set|apply|export> [OPTIONS]"
    );
}

#[test]
fn the_adapter_path_is_the_user_env_file() {
    assert!(adapter_path().ends_with("theme.env"));
}

#[test]
fn the_palette_has_every_role() {
    let palette = Palette::raven_violet();
    for role in Role::ALL {
        assert!(!palette.get(role).to_hex().is_empty());
    }
}

#[test]
fn apply_writes_the_adapters_and_reports_no_backup() {
    let dir = scratch("fresh");
    let path = dir.join("theme.env");
    let backup = apply_to(&path, &Palette::raven_violet(), Depth::Ansi256, false).expect("apply");
    assert!(backup.is_none());
    let text = fs::read_to_string(&path).expect("adapter file");
    assert!(text.contains("RAVEN_PALETTE='#b388ff:"));
    assert!(text.contains("LS_COLORS="));
    assert!(text.contains("LESS_TERMCAP_md="));
    assert!(!text.contains('\x1b'));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_backs_up_an_existing_file_before_replacing_it() {
    let dir = scratch("backup");
    let path = dir.join("theme.env");
    fs::write(&path, "old adapters\n").expect("old file");
    let backup = apply_to(&path, &Palette::raven_violet(), Depth::Ansi256, false).expect("apply");
    let backup = backup.expect("backup");
    assert_eq!(backup, path.with_file_name("theme.env.bak"));
    assert_eq!(
        fs::read_to_string(&backup).expect("backup"),
        "old adapters\n"
    );
    let text = fs::read_to_string(&path).expect("adapter file");
    assert!(text.contains("LS_COLORS="));
    assert_ne!(text, "old adapters\n");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_dry_run_lists_the_backup_and_changes_nothing() {
    let dir = scratch("dry-run");
    let path = dir.join("theme.env");
    fs::write(&path, "old adapters\n").expect("old file");
    let backup = apply_to(&path, &Palette::raven_violet(), Depth::Ansi256, true).expect("dry run");
    assert_eq!(backup, Some(path.with_file_name("theme.env.bak")));
    assert_eq!(
        fs::read_to_string(&path).expect("old file"),
        "old adapters\n"
    );
    assert!(!path.with_file_name("theme.env.bak").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_dry_run_on_a_fresh_path_plans_no_backup() {
    let dir = scratch("dry-fresh");
    let path = dir.join("theme.env");
    let backup = apply_to(&path, &Palette::raven_violet(), Depth::Ansi256, true).expect("dry run");
    assert!(backup.is_none());
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_backup_and_temporary_names_are_suffixed() {
    let path = Path::new("/tmp/theme.env");
    assert_eq!(backup_path(path), PathBuf::from("/tmp/theme.env.bak"));
    assert_eq!(appended(path, ".tmp"), PathBuf::from("/tmp/theme.env.tmp"));
}
