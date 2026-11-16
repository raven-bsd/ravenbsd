// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Initfs packing and uutils staging-detection tests.

use super::*;
use crate::profiles::{Boot, File, Initfs as InitfsSpec, Profile};
use raven_initfs::Initfs;

fn profile(files: &[(&str, &str)]) -> Profile {
    Profile {
        name: "test".to_string(),
        initfs: InitfsSpec {
            programs: Vec::new(),
            files: files
                .iter()
                .map(|(src, path)| File {
                    src: (*src).to_string(),
                    path: (*path).to_string(),
                    mode: 0o755,
                    generated: true,
                })
                .collect(),
        },
        boot: Boot {
            cmdline: "console=ttyS0".to_string(),
            log_level: "info".to_string(),
        },
    }
}

#[test]
fn packed_image_round_trips_through_the_reader() {
    let mut builder = InitfsBuilder::new();
    builder.add_file("/init", b"init-binary", 0o755).unwrap();
    builder
        .add_file("/bin/uname", b"uname-binary", 0o755)
        .unwrap();
    let image = builder.build();
    let parsed = Initfs::parse(&image).unwrap();
    assert_eq!(parsed.len(), 2);
    let init = parsed.get("/init").unwrap();
    assert_eq!(init.data, b"init-binary");
    assert_eq!(init.mode, 0o755);
    let uname = parsed.get("/bin/uname").unwrap();
    assert_eq!(uname.data, b"uname-binary");
}

#[test]
fn release_user_programs_requires_an_explicit_one() {
    assert!(release_user_programs_from_env(Some("1")));
    assert!(!release_user_programs_from_env(None));
    assert!(!release_user_programs_from_env(Some("true")));
    assert!(!release_user_programs_from_env(Some("0")));
}

#[test]
fn extra_file_keeps_its_mode_and_contents() {
    let mut builder = InitfsBuilder::new();
    builder.add_file("/etc/example", b"value\n", 0o640).unwrap();
    let image = builder.build();
    let parsed = Initfs::parse(&image).unwrap();
    let file = parsed.get("/etc/example").unwrap();
    assert_eq!(file.data, b"value\n");
    assert_eq!(file.mode, 0o640);
}

#[test]
fn only_the_matching_arch_uutils_tree_is_detected() {
    assert_eq!(
        uutils_source_name(Arch::X86_64, "build/x86_64/uutils/ls"),
        Some("ls")
    );
    assert_eq!(
        uutils_source_name(Arch::Aarch64, "build/aarch64/uutils/cat"),
        Some("cat")
    );
    assert_eq!(
        uutils_source_name(Arch::X86_64, "build/aarch64/uutils/ls"),
        None
    );
    assert_eq!(
        uutils_source_name(Arch::X86_64, "build/x86_64/uutils/"),
        None
    );
    assert_eq!(
        uutils_source_name(Arch::X86_64, "build/x86_64/uutils/sub/ls"),
        None
    );
    assert_eq!(uutils_source_name(Arch::X86_64, "services/ls.toml"), None);
}

#[test]
fn a_partial_uutils_tree_reports_only_the_missing_names() {
    let profile = profile(&[
        ("build/x86_64/uutils/ls", "/bin/ls"),
        ("build/x86_64/uutils/cat", "/bin/cat"),
        ("etc/example", "/etc/example"),
    ]);
    let exists = |source: &str| source.ends_with("/ls");
    assert_eq!(uutils_missing(&profile, Arch::X86_64, exists), ["cat"]);
}

#[test]
fn a_complete_uutils_tree_needs_no_build() {
    let profile = profile(&[("build/x86_64/uutils/ls", "/bin/ls")]);
    assert!(uutils_missing(&profile, Arch::X86_64, |_| true).is_empty());
}

#[test]
fn an_empty_uutils_tree_reports_every_required_name_once() {
    let profile = profile(&[
        ("build/x86_64/uutils/ls", "/bin/ls"),
        ("build/x86_64/uutils/cat", "/bin/cat"),
        ("build/x86_64/uutils/ls", "/bin/ls-again"),
    ]);
    assert_eq!(
        uutils_missing(&profile, Arch::X86_64, |_| false),
        ["ls", "cat"]
    );
}

#[test]
fn non_uutils_profile_files_do_not_trigger_the_slice_build() {
    let profile = profile(&[("etc/example", "/etc/example")]);
    assert!(uutils_missing(&profile, Arch::X86_64, |_| false).is_empty());
}
