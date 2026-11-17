// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the component build planner.

use super::*;

#[test]
fn kernel_target_is_unknown_none() {
    assert_eq!(
        target_for(Component::Kernel, Arch::X86_64).unwrap(),
        "x86_64-unknown-none"
    );
    assert_eq!(
        target_for(Component::Kernel, Arch::Aarch64).unwrap(),
        "aarch64-unknown-none-softfloat"
    );
    assert_eq!(
        target_for(Component::Kernel, Arch::Riscv64).unwrap(),
        "riscv64gc-unknown-none-elf"
    );
}

#[test]
fn user_targets_match_kernel_targets() {
    for arch in [Arch::X86_64, Arch::Aarch64, Arch::Riscv64] {
        assert_eq!(
            target_for(Component::User, arch).unwrap(),
            target_for(Component::Kernel, arch).unwrap()
        );
    }
}

#[test]
fn loader_targets_are_uefi() {
    assert_eq!(
        target_for(Component::Loader, Arch::X86_64).unwrap(),
        "x86_64-unknown-uefi"
    );
    assert_eq!(
        target_for(Component::Loader, Arch::Aarch64).unwrap(),
        "aarch64-unknown-uefi"
    );
}

#[test]
fn loader_has_no_riscv64_target() {
    assert!(target_for(Component::Loader, Arch::Riscv64).is_err());
}

#[test]
fn kernel_rustflags_match_c2() {
    let flags = rustflags_for(
        Component::Kernel,
        Arch::X86_64,
        Path::new("/r"),
        None,
        false,
    );
    assert_eq!(
        flags,
        vec![
            "-C",
            "relocation-model=static",
            "-C",
            "force-frame-pointers=yes",
            "--remap-path-prefix=/r=/ravenbsd",
        ]
    );
}

#[test]
fn user_rustflags_include_small_code_model_on_x86_64() {
    let linker = Path::new("/r/userland/linker/x86_64.ld");
    let flags = rustflags_for(
        Component::User,
        Arch::X86_64,
        Path::new("/r"),
        Some(linker),
        false,
    );
    assert_eq!(
        flags,
        vec![
            "-C",
            "code-model=small",
            "-C",
            "relocation-model=static",
            "-C",
            "force-frame-pointers=yes",
            "-C",
            "link-arg=-T/r/userland/linker/x86_64.ld",
            "--remap-path-prefix=/r=/ravenbsd",
        ]
    );
}

#[test]
fn user_rustflags_omit_missing_linker() {
    let flags = rustflags_for(Component::User, Arch::Aarch64, Path::new("/r"), None, false);
    assert!(!flags.iter().any(|flag| flag.contains("link-arg")));
    assert!(!flags.iter().any(|flag| flag == "code-model=small"));
}

#[test]
fn loader_rustflags_only_remap() {
    let flags = rustflags_for(
        Component::Loader,
        Arch::X86_64,
        Path::new("/r"),
        None,
        false,
    );
    assert_eq!(flags, vec!["--remap-path-prefix=/r=/ravenbsd"]);
}

#[test]
fn build_id_uses_the_commit_date() {
    assert_eq!(build_id_from(0, "abc1234", false), "abc1234 1970-01-01");
    assert_eq!(
        build_id_from(1_609_459_200, "abc1234", true),
        "abc1234-dirty 2021-01-01"
    );
}

#[test]
fn civil_date_handles_leap_day() {
    assert_eq!(civil_from_days(19_782), (2024, 2, 29));
}

#[test]
fn civil_date_handles_epoch() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(-1), (1969, 12, 31));
}

#[test]
fn artifact_paths_are_stable() {
    let root = Path::new("/repo");
    assert_eq!(
        artifact_path(root, Arch::X86_64, Component::Kernel, ""),
        Path::new("/repo/build/x86_64/kernel.elf")
    );
    assert_eq!(
        artifact_path(root, Arch::Aarch64, Component::Loader, ""),
        Path::new("/repo/build/aarch64/loader.efi")
    );
    assert_eq!(
        artifact_path(root, Arch::Riscv64, Component::User, "raven-init"),
        Path::new("/repo/build/riscv64/user/raven-init")
    );
}

#[test]
fn release_lto_keeps_fat_for_kernel_and_loader() {
    for arch in [Arch::X86_64, Arch::Aarch64] {
        assert_eq!(release_lto_for(Component::Kernel, arch), "fat");
        assert_eq!(release_lto_for(Component::Loader, arch), "fat");
    }
    assert_eq!(release_lto_for(Component::Kernel, Arch::Riscv64), "fat");
}

#[test]
fn release_lto_follows_the_code_model_table() {
    assert_eq!(code_model_for(Component::User, Arch::X86_64), Some("small"));
    assert_eq!(code_model_for(Component::User, Arch::Aarch64), None);
    assert_eq!(code_model_for(Component::User, Arch::Riscv64), None);
    assert_eq!(code_model_for(Component::Kernel, Arch::X86_64), None);
    assert_eq!(code_model_for(Component::Loader, Arch::X86_64), None);
}

#[test]
fn release_lto_falls_back_for_user_x86_64() {
    // Both fat and thin LTO reject the conflicting `Code Model` flag on this target, so the
    // DR-22 dynamic fallback is the only selection that links.
    assert_eq!(release_lto_for(Component::User, Arch::X86_64), "false");
    assert_eq!(release_lto_for(Component::User, Arch::Aarch64), "fat");
    assert_eq!(release_lto_for(Component::User, Arch::Riscv64), "fat");
}

#[test]
fn release_summary_names_the_code_model() {
    assert_eq!(
        release_summary(Component::User, Arch::X86_64),
        "lto=false (code-model=small)"
    );
    assert_eq!(release_summary(Component::Kernel, Arch::X86_64), "lto=fat");
    assert_eq!(release_summary(Component::Loader, Arch::Aarch64), "lto=fat");
}

#[test]
fn caller_profile_vars_are_dropped() {
    let vars = vec![
        ("PATH".to_string(), "/bin".to_string()),
        ("CARGO_PROFILE_RELEASE_LTO".to_string(), "fat".to_string()),
        ("CARGO_PROFILE_DEV_OPT_LEVEL".to_string(), "0".to_string()),
        ("RUSTFLAGS".to_string(), "-Cfoo".to_string()),
    ];
    assert_eq!(
        caller_profile_vars(vars),
        vec![
            "CARGO_PROFILE_DEV_OPT_LEVEL".to_string(),
            "CARGO_PROFILE_RELEASE_LTO".to_string(),
        ]
    );
}

#[test]
fn binary_name_defaults_to_the_package() {
    let manifest = "[package]\nname = \"raven-init\"\n";
    assert_eq!(binary_name(manifest, "raven-init").unwrap(), "raven-init");
}

#[test]
fn binary_name_reads_the_first_bin() {
    let manifest =
        "[package]\nname = \"raven-getty\"\n\n[[bin]]\nname = \"getty\"\npath = \"src/main.rs\"\n";
    assert_eq!(binary_name(manifest, "raven-getty").unwrap(), "getty");
}

#[test]
fn binary_name_rejects_a_bad_manifest() {
    assert!(binary_name("not toml [", "x").is_err());
}
