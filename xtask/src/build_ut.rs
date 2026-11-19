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
            "--cfg",
            "sha2_backend=\"soft\"",
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
fn dynamic_supported_is_x86_64_and_aarch64() {
    assert!(dynamic_supported(Arch::X86_64));
    assert!(dynamic_supported(Arch::Aarch64));
    assert!(!dynamic_supported(Arch::Riscv64));
}

#[test]
fn aarch64_dynamic_user_rustflags_are_pie() {
    let flags = rustflags_for(Component::User, Arch::Aarch64, Path::new("/r"), None, true);
    assert!(
        flags
            .windows(2)
            .any(|pair| pair == ["-C", "relocation-model=pic"])
    );
    assert!(!flags.iter().any(|flag| flag == "code-model=small"));
    assert!(
        flags
            .iter()
            .any(|flag| flag == "linker=/r/userland/linker/rust-lld-dynamic.sh")
    );
    for arg in [
        "link-arg=-pie",
        "link-arg=-Bdynamic",
        "link-arg=-dynamic-linker",
        "link-arg=/libexec/ld-elf.so.1",
        "link-arg=--image-base=0x400000",
        "link-arg=-z",
        "link-arg=max-page-size=4096",
        "link-arg=-L/r/build/aarch64/sysroot/lib",
        "link-arg=-L/r/build/aarch64/sysroot/usr/lib",
        "link-arg=--allow-shlib-undefined",
        "link-arg=--no-as-needed",
        "link-arg=-lc",
    ] {
        assert!(flags.iter().any(|flag| flag == arg), "missing {arg}");
    }
}

#[test]
fn x86_64_dynamic_user_rustflags_keep_the_small_model() {
    let flags = rustflags_for(Component::User, Arch::X86_64, Path::new("/r"), None, true);
    assert!(
        flags
            .windows(2)
            .any(|pair| pair == ["-C", "code-model=small"])
    );
    assert!(
        flags
            .windows(2)
            .any(|pair| pair == ["-C", "relocation-model=pic"])
    );
}

#[test]
fn dynamic_user_rustflags_pull_the_float_plugin() {
    for arch in [Arch::X86_64, Arch::Aarch64] {
        let flags = rustflags_for(Component::User, arch, Path::new("/r"), None, true);
        assert!(
            flags.windows(2).any(|pair| pair == ["-C", "link-arg=-u"]),
            "{arch:?} dynamic flags do not force a symbol"
        );
        assert!(
            flags
                .iter()
                .any(|flag| flag == "link-arg=__rl_fmt_float_impl"),
            "{arch:?} dynamic flags do not name the float plug-in"
        );
    }
}

#[test]
fn static_user_rustflags_leave_the_float_plugin_lazy() {
    let flags = rustflags_for(Component::User, Arch::X86_64, Path::new("/r"), None, false);
    assert!(
        !flags
            .iter()
            .any(|flag| flag.contains("__rl_fmt_float_impl")),
        "a static raven-rt program must not force the libc float plug-in"
    );
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

static NEXT_ELF: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn put16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn put_i64(data: &mut [u8], offset: usize, value: i64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[allow(clippy::too_many_arguments)] // an ELF program header is naturally eight fields
fn put_phdr(
    data: &mut [u8],
    index: usize,
    kind: u32,
    flags: u32,
    file_offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
) {
    let offset = 64 + index * 56;
    put32(data, offset, kind);
    put32(data, offset + 4, flags);
    put64(data, offset + 8, file_offset);
    put64(data, offset + 16, vaddr);
    put64(data, offset + 24, vaddr);
    put64(data, offset + 32, filesz);
    put64(data, offset + 40, memsz);
    put64(data, offset + 48, 0x1000);
}

/// A minimal ELF64/aarch64 PIE with one RELA target in the R-only segment and a TEXREL pair.
fn textrel_elf(machine: u16) -> Vec<u8> {
    let mut data = vec![0u8; 0x500];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    put16(&mut data, 18, machine);
    put64(&mut data, 0x20, 64);
    put16(&mut data, 0x36, 56);
    put16(&mut data, 0x38, 4);
    // R-only segment with the relocation slot at vaddr 0x400010.
    put_phdr(&mut data, 0, PT_LOAD, PF_R, 0x200, 0x400000, 0x100, 0x100);
    // R|E text segment; must stay RX.
    put_phdr(
        &mut data,
        1,
        PT_LOAD,
        PF_R | PF_X,
        0x300,
        0x401000,
        0x100,
        0x100,
    );
    // RW data segment containing the dynamic table.
    put_phdr(
        &mut data,
        2,
        PT_LOAD,
        PF_R | PF_W,
        0x400,
        0x402000,
        0x100,
        0x100,
    );
    put_phdr(
        &mut data,
        3,
        PT_DYNAMIC,
        PF_R | PF_W,
        0x400,
        0x402000,
        0x100,
        0x100,
    );
    // One R_AARCH64_RELATIVE-shaped RELA entry at vaddr 0x400010.
    put64(&mut data, 0x210, 0x400010);
    put64(&mut data, 0x218, 0);
    put_i64(&mut data, 0x220, 0);
    // Dynamic table: RELA at 0x400010, size 24, TEXTREL + DF_TEXTREL, NULL.
    let dynamic = 0x400;
    for (index, (tag, value)) in [
        (DT_RELA, 0x400010u64),
        (DT_RELASZ, 24),
        (DT_RELAENT, 24),
        (DT_FLAGS, DF_TEXTREL),
        (DT_TEXTREL, 0),
        (DT_NULL, 0),
    ]
    .iter()
    .enumerate()
    {
        let offset = dynamic + index * 16;
        put_i64(&mut data, offset, *tag);
        put64(&mut data, offset + 8, *value);
    }
    data
}

fn temp_elf(data: &[u8]) -> PathBuf {
    let id = NEXT_ELF.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("k97b-textrel-{}-{id}.elf", std::process::id()));
    fs::write(&path, data).unwrap();
    path
}

#[test]
fn aarch64_textrel_patch_makes_the_rodata_segment_writable() {
    let path = temp_elf(&textrel_elf(EM_AARCH64));
    patch_aarch64_textrel(&path).unwrap();
    let data = fs::read(&path).unwrap();
    assert_eq!(read_u32(&data, 64 + 4), Some(PF_R | PF_W));
    assert_eq!(read_u32(&data, 64 + 56 + 4), Some(PF_R | PF_X));
    let mut tags = Vec::new();
    let mut flags = 0;
    for index in 0..6 {
        let offset = 0x400 + index * 16;
        let tag = read_i64(&data, offset).unwrap_or(-1);
        tags.push(tag);
        if tag == DT_FLAGS {
            flags = read_u64(&data, offset + 8).unwrap_or(0);
        }
    }
    assert!(!tags.contains(&DT_TEXTREL));
    assert_eq!(flags & DF_TEXTREL, 0);
    fs::remove_file(&path).ok();
}

#[test]
fn aarch64_textrel_patch_is_idempotent() {
    let path = temp_elf(&textrel_elf(EM_AARCH64));
    patch_aarch64_textrel(&path).unwrap();
    let once = fs::read(&path).unwrap();
    patch_aarch64_textrel(&path).unwrap();
    let twice = fs::read(&path).unwrap();
    assert_eq!(once, twice);
    fs::remove_file(&path).ok();
}

#[test]
fn aarch64_textrel_patch_clears_flags_when_relocations_are_already_writable() {
    let mut data = textrel_elf(EM_AARCH64);
    // Move the relocation and DT_RELA into the RW segment; no segment flag needs changing.
    put64(&mut data, 0x210, 0x402010);
    put64(&mut data, 0x400 + 8, 0x402010);
    let path = temp_elf(&data);
    patch_aarch64_textrel(&path).unwrap();
    let data = fs::read(&path).unwrap();
    assert_eq!(read_u32(&data, 64 + 4), Some(PF_R));
    let mut tags = Vec::new();
    for index in 0..6 {
        tags.push(read_i64(&data, 0x400 + index * 16).unwrap_or(-1));
    }
    assert!(!tags.contains(&DT_TEXTREL));
    fs::remove_file(&path).ok();
}

#[test]
fn textrel_patch_ignores_non_aarch64_objects() {
    let path = temp_elf(&textrel_elf(62));
    patch_aarch64_textrel(&path).unwrap();
    let data = fs::read(&path).unwrap();
    assert_eq!(read_u32(&data, 64 + 4), Some(PF_R));
    fs::remove_file(&path).ok();
}

#[test]
fn release_image_profiles_are_explicit() {
    assert!(is_release_image_profile(Arch::X86_64, "release"));
    assert!(is_release_image_profile(Arch::Aarch64, "pi4-release"));
    for (arch, profile) in [
        (Arch::X86_64, "shell"),
        (Arch::X86_64, "base"),
        (Arch::X86_64, "release-test"),
        (Arch::X86_64, "pi4-release"),
        (Arch::Aarch64, "release"),
        (Arch::Aarch64, "pi4"),
        (Arch::Riscv64, "release"),
    ] {
        assert!(
            !is_release_image_profile(arch, profile),
            "{arch:?} profile {profile} must not trigger the release build-id guard"
        );
    }
}

#[test]
fn clean_release_build_id_is_accepted() {
    // Both embedded forms: x86_64 carries `<short> <date>`, the Pi build id has no date.
    assert!(verify_release_build_id(b") #1: abc1234 2026-11-17\0", "abc1234", false).is_ok());
    assert!(verify_release_build_id(b") abc1234\0", "abc1234", false).is_ok());
    // The token may start or end the inspected bytes.
    assert!(verify_release_build_id(b"abc1234", "abc1234", false).is_ok());
    assert!(verify_release_build_id(b"abc1234\0", "abc1234", false).is_ok());
    assert!(contains_clean_build_id(b"(abc1234)", "abc1234"));
}

#[test]
fn dirty_release_build_id_is_rejected() {
    assert!(verify_release_build_id(b") abc1234-dirty 2026-11-17\0", "abc1234", false).is_err());
    assert!(contains_dirty_build_id(b") abc1234-dirty\0", "abc1234"));
    let err = verify_release_build_id(b") abc1234 2026-11-17\0", "abc1234", true)
        .unwrap_err()
        .to_string();
    assert!(err.contains("clean worktree"), "{err}");
}

#[test]
fn stale_release_build_id_is_rejected() {
    let err = verify_release_build_id(b") def5678 2026-11-17\0", "abc1234", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("does not embed the clean HEAD id"), "{err}");
}

#[test]
fn longer_hashes_do_not_satisfy_the_clean_id() {
    // The clean short id must be a whole token, not a prefix or a substring of a longer hash.
    assert!(!contains_clean_build_id(b") abc12345\0", "abc1234"));
    assert!(!contains_clean_build_id(b") 0abc1234\0", "abc1234"));
    assert!(!contains_clean_build_id(b") abc1234-dirty\0", "abc1234"));
    assert!(!contains_clean_build_id(b"", "abc1234"));
    assert!(!contains_clean_build_id(b"abc1234", ""));
}

#[test]
fn invalid_short_id_is_rejected() {
    assert!(verify_release_build_id(b"abc1234", "", false).is_err());
    assert!(verify_release_build_id(b"abc1234", "not-hex!", false).is_err());
}

#[test]
fn profile_artifact_names_keep_release_and_debug_apart() {
    assert_eq!(
        artifact_filename(Component::Kernel, true),
        Some("kernel.elf")
    );
    assert_eq!(
        artifact_filename(Component::Kernel, false),
        Some("kernel-debug.elf")
    );
    assert_eq!(
        artifact_filename(Component::Loader, true),
        Some("loader.efi")
    );
    assert_eq!(
        artifact_filename(Component::Loader, false),
        Some("loader-debug.efi")
    );
    assert_eq!(artifact_filename(Component::User, true), None);
    assert_eq!(
        profile_artifact_path(Path::new("/r"), Arch::X86_64, Component::Kernel, false),
        Path::new("/r/build/x86_64/kernel-debug.elf")
    );
}

#[test]
fn artifact_profile_stamp_round_trips_and_rejects_mismatch() {
    let root = std::env::temp_dir().join(format!("raven-artifact-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let kernel = root.join("kernel.elf");
    fs::write(&kernel, b"kernel").unwrap();
    write_artifact_profile(&kernel, true).unwrap();
    assert_eq!(read_artifact_profile(&kernel), Some(true));
    assert!(verify_artifact_profile(&kernel, true).is_ok());
    assert!(verify_artifact_profile(&kernel, false).is_err());
    assert_eq!(parse_artifact_profile(b"release\n"), Some(true));
    assert_eq!(parse_artifact_profile(b"debug\n"), Some(false));
    assert_eq!(parse_artifact_profile(b"other\n"), None);
    let unstamped = root.join("loader.efi");
    fs::write(&unstamped, b"loader").unwrap();
    assert!(verify_artifact_profile(&unstamped, true).is_err());
    let _ = fs::remove_dir_all(&root);
}
