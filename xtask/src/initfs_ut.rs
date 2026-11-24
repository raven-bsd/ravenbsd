// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Initfs packing and uutils staging-detection tests.

use super::*;
use crate::profiles::{Boot, File, Image, Initfs as InitfsSpec, Profile, Program};
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
            release: false,
            dynamic: false,
        },
        image: Image::default(),
        boot: Boot {
            cmdline: "console=ttyS0".to_string(),
            log_level: "info".to_string(),
        },
    }
}

fn program(package: &str, path: &str) -> Program {
    Program {
        package: package.to_string(),
        path: path.to_string(),
    }
}

#[test]
fn shell_bin_tools_link_statically_but_getty_and_login_stay_dynamic() {
    for package in [
        "curl",
        "man",
        "raven-date",
        "raven-hostname",
        "raven-ps",
        "raven-rsh",
        "raven-sleep",
        "raven-svcctl",
        "raven-uname",
        "wget",
    ] {
        assert!(
            program_is_static(&program(package, "/bin/tool")),
            "{package} must be static"
        );
    }
    assert!(!program_is_static(&program("raven-getty", "/bin/getty")));
    assert!(!program_is_static(&program("login", "/bin/login")));
    assert!(!program_is_static(&program("raven-other", "/bin/other")));
    assert!(program_is_static(&program("raven-vfsd", "/sbin/vfsd")));
    assert!(program_is_static(&program("raven-init", "/init")));
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
fn release_user_programs_follow_the_profile_without_an_override() {
    assert!(release_user_programs(true, None));
    assert!(!release_user_programs(false, None));
}

#[test]
fn release_user_programs_env_overrides_the_profile() {
    assert!(release_user_programs(false, Some("1")));
    assert!(!release_user_programs(true, Some("0")));
    // Unrecognised values do not silently override the profile.
    assert!(release_user_programs(true, Some("true")));
    assert!(!release_user_programs(false, Some("true")));
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
    assert_eq!(
        uutils_source_name(Arch::Aarch64, "build/{arch}/uutils/ls"),
        Some("ls")
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

#[test]
fn libc_outputs_are_arch_specific_and_ordered() {
    assert_eq!(
        libc_missing(Arch::X86_64, false, |_| false),
        [
            "build/x86_64/libc.a",
            "build/x86_64/crt1.o",
            "build/x86_64/crti.o",
            "build/x86_64/crtn.o"
        ]
    );
    assert_eq!(
        libc_missing(Arch::Aarch64, false, |_| false),
        [
            "build/aarch64/libc.a",
            "build/aarch64/crt1.o",
            "build/aarch64/crti.o",
            "build/aarch64/crtn.o"
        ]
    );
    assert!(libc_missing(Arch::X86_64, false, |_| true).is_empty());
}

#[test]
fn a_partial_libc_pack_reports_only_the_missing_output() {
    let missing = libc_missing(Arch::X86_64, false, |path| !path.ends_with("crt1.o"));
    assert_eq!(missing, ["build/x86_64/crt1.o"]);
}

#[test]
fn dynamic_stage_files_cover_x86_64_and_aarch64() {
    assert_eq!(
        dynamic_stage_files(Arch::X86_64, true),
        [
            ("/lib/libc.so.1", "build/x86_64/libc.so.1".to_string()),
            (
                "/libexec/ld-elf.so.1",
                "build/x86_64/ld-elf.so.1".to_string()
            ),
        ]
    );
    assert_eq!(
        dynamic_stage_files(Arch::Aarch64, true),
        [
            ("/lib/libc.so.1", "build/aarch64/libc.so.1".to_string()),
            (
                "/libexec/ld-elf.so.1",
                "build/aarch64/ld-elf.so.1".to_string()
            ),
        ]
    );
    assert!(dynamic_stage_files(Arch::X86_64, false).is_empty());
    assert!(dynamic_stage_files(Arch::Aarch64, false).is_empty());
    assert!(dynamic_stage_files(Arch::Riscv64, true).is_empty());
}

#[test]
fn a_dynamic_profile_requires_the_loader_and_libc_prerequisites() {
    let mut profile = profile(&[]);
    profile.initfs.dynamic = true;
    let plan = prerequisite_plan(&profile, Arch::X86_64, |_| false);
    assert_eq!(
        plan.libc,
        [
            "build/x86_64/libc.a",
            "build/x86_64/crt1.o",
            "build/x86_64/crti.o",
            "build/x86_64/crtn.o",
            "build/x86_64/libc.so.1",
            "build/x86_64/ld-elf.so.1",
        ]
    );
    let aarch64 = prerequisite_plan(&profile, Arch::Aarch64, |_| false);
    assert!(
        aarch64
            .libc
            .contains(&"build/aarch64/libc.so.1".to_string())
    );
    assert!(
        aarch64
            .libc
            .contains(&"build/aarch64/ld-elf.so.1".to_string())
    );
    let riscv = prerequisite_plan(&profile, Arch::Riscv64, |_| false);
    assert!(!riscv.libc.iter().any(|path| path.ends_with(".so.1")));
}

#[test]
fn legacy_and_arch_brush_sources_select_the_arch_path() {
    assert!(is_brush_source("build/brush", Arch::X86_64));
    assert!(is_brush_source("build/x86_64/brush", Arch::X86_64));
    assert!(!is_brush_source("build/aarch64/brush", Arch::X86_64));
    assert!(!is_brush_source("build/x86_64/brush", Arch::Aarch64));
    assert!(!is_brush_source("build/x86_64/uutils/ls", Arch::X86_64));
    assert!(!is_brush_source("etc/profile", Arch::X86_64));
}

#[test]
fn brush_is_requested_only_when_the_profile_stages_it() {
    let legacy = profile(&[("build/brush", "/bin/brush")]);
    assert!(brush_requested(&legacy, Arch::X86_64));
    let arch = profile(&[("build/x86_64/brush", "/bin/brush")]);
    assert!(brush_requested(&arch, Arch::X86_64));
    assert!(!brush_requested(&arch, Arch::Aarch64));
    let none = profile(&[("etc/profile", "/etc/profile")]);
    assert!(!brush_requested(&none, Arch::X86_64));
}

#[test]
fn brush_missing_checks_the_arch_path_not_the_legacy_source() {
    let legacy = profile(&[("build/brush", "/bin/brush")]);
    assert!(brush_missing(&legacy, Arch::X86_64, |_| false));
    assert!(!brush_missing(&legacy, Arch::X86_64, |path| {
        path == "build/x86_64/brush"
    }));
    assert!(brush_missing(&legacy, Arch::X86_64, |path| {
        // A stale mutable `build/brush` must not satisfy the prerequisite.
        path == "build/brush"
    }));
}

#[test]
fn source_path_maps_the_arch_placeholder_and_legacy_brush() {
    let root = std::path::Path::new("/repo");
    assert_eq!(
        source_path(root, Arch::X86_64, "build/brush"),
        std::path::PathBuf::from("/repo/build/x86_64/brush")
    );
    assert_eq!(
        source_path(root, Arch::Aarch64, "build/brush"),
        std::path::PathBuf::from("/repo/build/aarch64/brush")
    );
    assert_eq!(
        source_path(root, Arch::X86_64, "build/x86_64/brush"),
        std::path::PathBuf::from("/repo/build/x86_64/brush")
    );
    assert_eq!(
        source_path(root, Arch::Aarch64, "build/{arch}/uutils/ls"),
        std::path::PathBuf::from("/repo/build/aarch64/uutils/ls")
    );
    assert_eq!(
        source_path(root, Arch::X86_64, "build/{arch}/uutils/ls"),
        std::path::PathBuf::from("/repo/build/x86_64/uutils/ls")
    );
    assert_eq!(
        source_path(root, Arch::X86_64, "tests/brush/bash-compat.sh"),
        std::path::PathBuf::from("/repo/tests/brush/bash-compat.sh")
    );
}

#[test]
fn uutils_requested_detects_staged_sources_only() {
    let staged = profile(&[("build/x86_64/uutils/ls", "/bin/ls")]);
    assert!(uutils_requested(&staged, Arch::X86_64));
    assert!(!uutils_requested(&staged, Arch::Aarch64));
    let other = profile(&[("etc/profile", "/etc/profile")]);
    assert!(!uutils_requested(&other, Arch::X86_64));
}

#[test]
fn prerequisite_plan_lists_every_missing_step_once() {
    let staged = profile(&[
        ("build/brush", "/bin/brush"),
        ("build/x86_64/uutils/ls", "/bin/ls"),
        ("build/x86_64/uutils/cat", "/bin/cat"),
        ("build/x86_64/uutils/ls", "/bin/ls-again"),
        ("etc/profile", "/etc/profile"),
    ]);
    let plan = prerequisite_plan(&staged, Arch::X86_64, |_| false);
    assert_eq!(
        plan.libc,
        [
            "build/x86_64/libc.a",
            "build/x86_64/crt1.o",
            "build/x86_64/crti.o",
            "build/x86_64/crtn.o"
        ]
    );
    assert!(plan.brush);
    assert_eq!(plan.uutils, ["ls", "cat"]);

    let complete = prerequisite_plan(&staged, Arch::X86_64, |_| true);
    assert!(complete.libc.is_empty());
    assert!(!complete.brush);
    assert!(complete.uutils.is_empty());

    let no_prerequisites = profile(&[("etc/profile", "/etc/profile")]);
    let plan = prerequisite_plan(&no_prerequisites, Arch::X86_64, |_| false);
    assert!(plan.libc.is_empty());
    assert!(!plan.brush);
    assert!(plan.uutils.is_empty());
}

#[test]
fn missing_fixtures_lists_each_directory_once() {
    let profile = profile(&[
        ("build/fixtures/pi4-release/one.dtb", "/one"),
        ("build/fixtures/pi4-release/other/two.dtb", "/two"),
    ]);
    let missing = missing_fixtures(&profile, |_| false);
    assert_eq!(missing, ["pi4-release"]);
}

#[test]
fn missing_fixtures_skips_existing_sources() {
    let profile = profile(&[("build/fixtures/pi4-release/one.dtb", "/one")]);
    assert!(missing_fixtures(&profile, |_| true).is_empty());
}

#[test]
fn release_userspace_paths_keep_the_cargo_target_caches() {
    let root = Path::new("/repo");
    let x86 = release_userspace_paths(root, Arch::X86_64);
    assert_eq!(
        x86,
        [
            PathBuf::from("/repo/build/x86_64/brush"),
            PathBuf::from("/repo/build/x86_64/uutils"),
            PathBuf::from("/repo/build/x86_64/libc.so.1"),
            PathBuf::from("/repo/build/x86_64/ld-elf.so.1"),
            PathBuf::from("/repo/build/x86_64/loader-manifest.toml"),
        ]
    );
    assert!(!x86.iter().any(|path| path.starts_with("/repo/build/cargo")));
    let arm = release_userspace_paths(root, Arch::Aarch64);
    assert_eq!(
        arm,
        [
            PathBuf::from("/repo/build/aarch64/brush"),
            PathBuf::from("/repo/build/aarch64/uutils"),
            PathBuf::from("/repo/build/aarch64/libc.so.1"),
            PathBuf::from("/repo/build/aarch64/ld-elf.so.1"),
            PathBuf::from("/repo/build/aarch64/loader-manifest.toml"),
        ]
    );
    assert!(!arm.iter().any(|path| path.starts_with("/repo/build/cargo")));
}

#[test]
fn release_purge_removes_staged_outputs_and_keeps_target_caches() {
    let root = std::env::temp_dir().join(format!("raven-k131-purge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("build/x86_64/uutils")).unwrap();
    std::fs::create_dir_all(root.join("build/cargo/brush-x86_64")).unwrap();
    std::fs::create_dir_all(root.join("build/cargo/uutils-x86_64")).unwrap();
    std::fs::write(root.join("build/x86_64/brush"), b"stale").unwrap();
    std::fs::write(root.join("build/x86_64/uutils/ls"), b"stale").unwrap();
    std::fs::write(root.join("build/cargo/brush-x86_64/artifact"), b"warm").unwrap();
    std::fs::write(root.join("build/cargo/uutils-x86_64/artifact"), b"warm").unwrap();
    // The K131 stale shape: a non-ELF loader plus a manifest from another build.
    std::fs::write(root.join("build/x86_64/ld-elf.so.1"), b"STALE-NOT-ELF").unwrap();
    std::fs::write(root.join("build/x86_64/libc.so.1"), b"stale libc").unwrap();
    std::fs::write(root.join("build/x86_64/loader-manifest.toml"), b"stale").unwrap();

    purge_release_userspace(&root, Arch::X86_64).unwrap();
    for path in release_userspace_paths(&root, Arch::X86_64) {
        assert!(!path.exists(), "{} still exists", path.display());
    }
    // The cargo target caches are reused: cargo fingerprints features and RUSTFLAGS, and the
    // staged outputs are rebuilt from them, so a stale image binary still cannot be packed.
    assert!(root.join("build/cargo/brush-x86_64/artifact").exists());
    assert!(root.join("build/cargo/uutils-x86_64/artifact").exists());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn libc_stamp_tracks_the_source_hash() {
    let root = std::env::temp_dir().join(format!("raven-k548-libc-stamp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("libc/ravenlibc/crates/raven-libc/src.rs");
    std::fs::create_dir_all(source.parent().expect("parent")).expect("mkdir");
    std::fs::write(&source, b"one").expect("write");
    let stamp = root.join("build/x86_64/libc.stamp");
    std::fs::create_dir_all(stamp.parent().expect("parent")).expect("mkdir");
    let hash = crate::libc_pack::source_hash(&root).expect("hash");
    std::fs::write(
        &stamp,
        format!("format=k441.1\narch=x86_64\nset=server\nsources={hash:016x}\n"),
    )
    .expect("stamp");
    assert!(libc_stamp_current(&root, Arch::X86_64));
    std::fs::write(&source, b"two").expect("rewrite");
    assert!(!libc_stamp_current(&root, Arch::X86_64));
    std::fs::remove_file(&stamp).expect("remove stamp");
    assert!(!libc_stamp_current(&root, Arch::X86_64));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn source_path_resolves_arch_in_any_position() {
    let root = Path::new("/repo");
    assert_eq!(
        source_path(root, Arch::Aarch64, "build/ssh/{arch}/bin/sshd"),
        root.join("build/ssh/aarch64/bin/sshd")
    );
    assert_eq!(
        source_path(root, Arch::X86_64, "build/{arch}/uutils/ls"),
        root.join("build/x86_64/uutils/ls")
    );
}

fn elf_header(machine: u16) -> Vec<u8> {
    let mut data = vec![0u8; 64];
    data[..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    data[6] = 1;
    data[16..18].copy_from_slice(&2u16.to_le_bytes());
    data[18..20].copy_from_slice(&machine.to_le_bytes());
    data
}

#[test]
fn foreign_arch_elf_is_rejected_for_aarch64() {
    let data = elf_header(62);
    let error = verify_staged_elf(Arch::Aarch64, Path::new("build/ssh/x86_64/bin/sshd"), &data)
        .expect_err("a foreign-arch ELF must not be staged");
    let text = format!("{error:#}");
    assert!(text.contains("BadMachine"), "{text}");
    assert!(text.contains("aarch64"), "{text}");
}

#[test]
fn matching_arch_elf_is_accepted() {
    let data = elf_header(183);
    assert!(
        verify_staged_elf(
            Arch::Aarch64,
            Path::new("build/ssh/aarch64/bin/sshd"),
            &data
        )
        .is_ok()
    );
}

#[test]
fn non_elf_staged_file_is_ignored() {
    assert!(verify_staged_elf(Arch::Aarch64, Path::new("etc/rc.conf"), b"sshd=YES\n").is_ok());
}
