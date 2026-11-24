// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Addon freshness tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::addon_of;
use super::stale;

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("raven-addons-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("temp root");
    root
}

fn write_at(path: &Path, seconds: u64) {
    write_bytes_at(path, b"x", seconds);
}

fn write_bytes_at(path: &Path, bytes: &[u8], seconds: u64) {
    fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    fs::write(path, bytes).expect("write");
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
    let times = fs::FileTimes::new().set_modified(modified);
    fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_times(times)
        .expect("set mtime");
}

#[test]
fn addon_of_parses_the_staged_addon_name() {
    assert_eq!(addon_of("build/ssh/x86_64/bin/sshd"), Some("ssh"));
    assert_eq!(
        addon_of("build/ssh/x86_64/libexec/sshd-session"),
        Some("ssh")
    );
    assert_eq!(addon_of("build/cargo/uutils/foo"), None);
    assert_eq!(addon_of("build/fixtures/x/y"), None);
    assert_eq!(addon_of("etc/passwd"), None);
}

#[test]
fn stale_detects_a_newer_addon_source() {
    let root = temp_root("source");
    let binary = root.join("build/ssh/x86_64/bin/sshd");
    let source = root.join("addons/ssh/openssh/session.c");
    write_at(&binary, 100);
    write_at(&source, 200);
    assert!(stale(&binary, &[source]));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stale_detects_a_newer_libc_archive() {
    let root = temp_root("libc");
    let binary = root.join("build/ssh/x86_64/bin/sshd");
    let libc = root.join("build/x86_64/libc.a");
    write_at(&binary, 100);
    write_at(&libc, 300);
    assert!(stale(&binary, &[libc]));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_rebuilt_binary_newer_than_its_inputs_is_current() {
    let root = temp_root("fresh");
    let binary = root.join("build/ssh/x86_64/bin/sshd");
    let source = root.join("addons/ssh/openssh/session.c");
    let libc = root.join("build/x86_64/libc.a");
    write_at(&binary, 300);
    write_at(&source, 100);
    write_at(&libc, 200);
    assert!(!stale(&binary, &[source, libc]));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn identical_libc_content_ignores_a_newer_mtime() {
    let root = temp_root("same-content");
    let binary = root.join("build/ssh/x86_64/bin/sshd");
    let stage = root.join("build/x86_64");
    let libc = stage.join("libc.a");
    let sysroot_lib = root.join("build/ssh/x86_64/sysroot/usr/lib");
    let sysroot = sysroot_lib.join("libc.a");
    write_at(&binary, 100);
    write_bytes_at(&libc, b"same-bytes", 300);
    write_bytes_at(&sysroot, b"same-bytes", 100);
    assert_eq!(
        super::newer_material_input(&binary, std::slice::from_ref(&libc), &stage, &sysroot_lib),
        None
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn different_libc_content_with_a_newer_mtime_is_stale() {
    let root = temp_root("diff-content");
    let binary = root.join("build/ssh/x86_64/bin/sshd");
    let stage = root.join("build/x86_64");
    let libc = stage.join("libc.a");
    let sysroot_lib = root.join("build/ssh/x86_64/sysroot/usr/lib");
    let sysroot = sysroot_lib.join("libc.a");
    write_at(&binary, 100);
    write_bytes_at(&libc, b"new-bytes", 300);
    write_bytes_at(&sysroot, b"old-bytes", 100);
    assert_eq!(
        super::newer_material_input(&binary, std::slice::from_ref(&libc), &stage, &sysroot_lib),
        Some(libc)
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_newer_addon_source_outside_the_libc_stage_is_still_stale() {
    let root = temp_root("source-material");
    let binary = root.join("build/ssh/x86_64/bin/sshd");
    let source = root.join("addons/ssh/openssh/session.c");
    let stage = root.join("build/x86_64");
    let sysroot_lib = root.join("build/ssh/x86_64/sysroot/usr/lib");
    write_at(&binary, 100);
    write_at(&source, 300);
    assert_eq!(
        super::newer_material_input(&binary, std::slice::from_ref(&source), &stage, &sysroot_lib),
        Some(source)
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn mutation_without_the_inputs_passes_the_stale_binary() {
    // Negative control for the gate: the same stale binary is accepted when the input list is
    // emptied. The production path always supplies the addon sources and libc archive, so this
    // is the "stale pass" the gate removes.
    let root = temp_root("mutation");
    let binary = root.join("build/ssh/x86_64/bin/sshd");
    let source = root.join("addons/ssh/openssh/session.c");
    write_at(&binary, 100);
    write_at(&source, 300);
    assert!(stale(&binary, std::slice::from_ref(&source)));
    assert!(!stale(&binary, &[]));
    let _ = fs::remove_dir_all(&root);
}
