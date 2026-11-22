// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Command-vector tests of `libc-pack` (C60 section 6 step 4). No process is spawned: the
//! tests compare the parsed `Options` and the exact `CommandSpec` the pipeline would run.

use std::path::Path;

use super::{
    Arch, apply_arch_waivers, build_spec, clean_compiler_headers, copy_headers, is_compiler_header,
    parse, source_hash,
};
use crate::process::CommandSpec;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn defaults_are_x86_64_server_and_build_dir() {
    let options = parse(&[]).expect("empty vector");
    assert_eq!(options.arch, Arch::X86_64);
    assert_eq!(options.set, "server");
    assert!(!options.check);
    assert!(options.probe.is_none());
    assert!(!options.host_run);
    assert!(!options.json);
    assert_eq!(options.out, Path::new("build/x86_64"));
}

#[test]
fn every_flag_parses_in_any_order() {
    let options = parse(&args(&[
        "--json",
        "--out",
        "target/pack",
        "--host-run",
        "--probe",
        "all",
        "--check",
        "--set",
        "tiny",
        "--arch",
        "aarch64",
        "--verify",
    ]))
    .expect("all flags");
    assert_eq!(options.arch, Arch::Aarch64);
    assert_eq!(options.set, "tiny");
    assert!(options.check);
    assert!(options.verify);
    assert_eq!(options.probe.as_deref(), Some("all"));
    assert!(options.host_run);
    assert!(options.json);
    assert_eq!(options.out, Path::new("target/pack"));
}

#[test]
fn the_short_name_drives_the_default_out_dir() {
    for (arch, dir) in [
        (Arch::X86_64, "build/x86_64"),
        (Arch::Aarch64, "build/aarch64"),
        (Arch::Riscv64, "build/riscv64"),
    ] {
        let options = parse(&args(&["--arch", arch.short()])).expect("arch");
        assert_eq!(options.out, Path::new(dir));
        assert_eq!(options.arch, arch);
    }
}

#[test]
fn every_unknown_arch_is_rejected() {
    for value in ["sparc64", "X86_64", "riscv", ""] {
        assert!(parse(&args(&["--arch", value])).is_err(), "{value}");
    }
}

#[test]
fn every_unknown_set_is_rejected() {
    for value in ["base", "Server", ""] {
        assert!(parse(&args(&["--set", value])).is_err(), "{value}");
    }
}

#[test]
fn every_flag_without_its_value_is_rejected() {
    for flag in ["--arch", "--set", "--probe", "--out"] {
        assert!(parse(&args(&[flag])).is_err(), "{flag}");
    }
}

#[test]
fn an_unknown_flag_is_rejected() {
    assert!(parse(&args(&["--arch=x86_64"])).is_err());
    assert!(parse(&args(&["--clean"])).is_err());
    assert!(parse(&args(&["x86_64"])).is_err());
}

#[test]
fn source_hash_tracks_content_not_mtime() {
    let dir = std::env::temp_dir().join(format!("k441-source-hash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let source = dir.join("libc/ravenlibc/crates/raven-libc/src/lib.rs");
    std::fs::create_dir_all(source.parent().expect("parent")).expect("mkdir");
    std::fs::write(&source, b"one").expect("write");
    std::fs::write(dir.join("Cargo.lock"), b"lock").expect("lock");
    std::fs::write(dir.join("rust-toolchain.toml"), b"toolchain").expect("toolchain");
    let first = source_hash(&dir).expect("first hash");
    std::fs::write(&source, b"two").expect("rewrite");
    let second = source_hash(&dir).expect("second hash");
    assert_ne!(first, second);
    std::fs::write(&source, b"two").expect("same content");
    assert_eq!(second, source_hash(&dir).expect("stable hash"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn x86_64_build_spec_has_the_exact_cargo_vector() {
    let spec = build_spec(
        Path::new("/repo"),
        Arch::X86_64,
        "libc-a-server",
        "server",
        Path::new("build/x86_64/cargo/libc-a"),
    );
    let expected = CommandSpec::new(
        "cargo",
        vec![
            "rustc",
            "-p",
            "raven-libc",
            "--crate-type",
            "staticlib",
            "--profile",
            "libc-a-server",
            "--target",
            "x86_64-unknown-none",
            "--target-dir",
            "build/x86_64/cargo/libc-a",
            "--features",
            "server",
            "--",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        "/repo",
    )
    .env(
        "RUSTFLAGS",
        "-C relocation-model=pic -C code-model=small --remap-path-prefix=/repo=/ravenbsd",
    )
    .env_remove("CARGO_ENCODED_RUSTFLAGS");
    assert_eq!(spec.program, expected.program);
    assert_eq!(spec.args, expected.args);
    assert_eq!(spec.env, expected.env);
    assert_eq!(spec.env_remove, expected.env_remove);
    assert_eq!(spec.cwd, expected.cwd);
}

#[test]
fn aarch64_build_spec_drops_the_x86_only_code_model_flag() {
    let spec = build_spec(
        Path::new("/repo"),
        Arch::Aarch64,
        "libc-a",
        "tiny",
        Path::new("out"),
    );
    assert!(
        spec.args
            .contains(&"aarch64-unknown-none-softfloat".to_string())
    );
    assert!(spec.args.contains(&"tiny".to_string()));
    let flags = spec
        .env
        .iter()
        .find(|(key, _)| key == "RUSTFLAGS")
        .map(|(_, value)| value.as_str())
        .expect("RUSTFLAGS");
    assert!(!flags.contains("code-model"));
    assert!(flags.contains("relocation-model=pic"));
    assert!(flags.contains("lto=thin"));
    assert!(flags.contains("embed-bitcode=yes"));
    assert_eq!(spec.env_remove, vec!["CARGO_ENCODED_RUSTFLAGS".to_string()]);
}

#[test]
fn riscv64_build_spec_targets_the_bare_metal_triple() {
    let spec = build_spec(
        Path::new("/repo"),
        Arch::Riscv64,
        "libc-a",
        "server",
        Path::new("out"),
    );
    assert!(
        spec.args
            .contains(&"riscv64gc-unknown-none-elf".to_string())
    );
    let flags = spec
        .env
        .iter()
        .find(|(key, _)| key == "RUSTFLAGS")
        .map(|(_, value)| value.as_str())
        .expect("RUSTFLAGS");
    assert!(flags.contains("relocation-model=pic"));
    assert!(flags.contains("lto=thin"));
    assert!(flags.contains("embed-bitcode=yes"));
    assert_eq!(spec.program, "cargo");
    assert_eq!(spec.cwd, Path::new("/repo"));
}

#[test]
fn copy_headers_creates_nested_destination_directories() {
    let root = std::env::temp_dir().join(format!(
        "c74b-copy-headers-{}-{}",
        std::process::id(),
        line!()
    ));
    let source = root.join("source");
    let target = root.join("target");
    std::fs::create_dir_all(source.join("arpa")).expect("source dir");
    std::fs::write(source.join("arpa/inet.h"), b"#pragma once\n").expect("nested header");
    std::fs::write(source.join("top.h"), b"#pragma once\n").expect("top header");
    std::fs::create_dir_all(&target).expect("target dir");

    copy_headers(&source, &target).expect("copy headers");

    assert!(target.join("arpa/inet.h").is_file());
    assert!(target.join("top.h").is_file());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn copy_headers_skips_compiler_provided_wrappers() {
    let root = std::env::temp_dir().join(format!(
        "c74c-copy-headers-{}-{}",
        std::process::id(),
        line!()
    ));
    let source = root.join("source");
    let target = root.join("target");
    std::fs::create_dir_all(&source).expect("source dir");
    std::fs::write(source.join("stdatomic.h"), b"#include_next <stdatomic.h>\n").expect("wrapper");
    std::fs::write(source.join("stdio.h"), b"#pragma once\n").expect("normal header");
    std::fs::create_dir_all(&target).expect("target dir");

    copy_headers(&source, &target).expect("copy headers");

    assert!(!target.join("stdatomic.h").exists());
    assert!(target.join("stdio.h").is_file());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn clean_compiler_headers_removes_stale_wrappers() {
    let root = std::env::temp_dir().join(format!(
        "c74c-clean-headers-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&root).expect("dir");
    std::fs::write(root.join("stdatomic.h"), b"#include_next <stdatomic.h>\n").expect("wrapper");
    std::fs::write(root.join("stdio.h"), b"#pragma once\n").expect("normal header");

    clean_compiler_headers(&root);

    assert!(!root.join("stdatomic.h").exists());
    assert!(root.join("stdio.h").is_file());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn only_the_compiler_wrappers_are_skipped() {
    for name in [
        "stddef.h",
        "stdbool.h",
        "stdatomic.h",
        "stdarg.h",
        "float.h",
    ] {
        assert!(is_compiler_header(name), "{name}");
    }
    for name in ["stdio.h", "stdlib.h", "time.h", "stdatomic_extra.h"] {
        assert!(!is_compiler_header(name), "{name}");
    }
}

#[test]
fn the_probe_value_is_kept_verbatim() {
    let options = parse(&args(&["--probe", "P0"])).expect("probe");
    assert_eq!(options.probe.as_deref(), Some("P0"));
    let options = parse(&args(&["--probe", "all"])).expect("probe");
    assert_eq!(options.probe.as_deref(), Some("all"));
}

#[test]
fn riscv64_waives_only_the_missing_long_double_parsers() {
    let root =
        std::env::temp_dir().join(format!("c74hs-waiver-{}-{}", std::process::id(), line!()));
    std::fs::create_dir_all(&root).expect("dir");
    let list = root.join("Symbols.list");
    let map = root.join("libc.map");
    std::fs::write(&list, "# header\nprintf\nstrtold\nwcstold\n").expect("list");
    std::fs::write(
        &map,
        "RAVEN_1.0 {\n\tglobal:\n\t\tprintf;\n\t\tstrtold;\n\t\twcstold;\n};\n",
    )
    .expect("map");

    let removed = apply_arch_waivers(&list, &map, Arch::Riscv64).expect("waive");
    assert_eq!(removed, ["strtold", "wcstold"]);
    let list_text = std::fs::read_to_string(&list).expect("read list");
    assert!(list_text.contains("printf"));
    assert!(!list_text.contains("strtold"));
    assert!(!list_text.contains("wcstold"));
    let map_text = std::fs::read_to_string(&map).expect("read map");
    assert!(map_text.contains("printf;"));
    assert!(!map_text.contains("strtold;"));
    assert!(!map_text.contains("wcstold;"));

    std::fs::write(&list, "printf\nstrtold\nwcstold\n").expect("list reset");
    std::fs::write(&map, "\t\tprintf;\n\t\tstrtold;\n\t\twcstold;\n").expect("map reset");
    assert!(
        apply_arch_waivers(&list, &map, Arch::X86_64)
            .expect("x86 no-op")
            .is_empty()
    );
    assert!(
        std::fs::read_to_string(&list)
            .expect("read list")
            .contains("strtold")
    );
    let _ = std::fs::remove_dir_all(&root);
}
