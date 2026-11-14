// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the probe table and the four command vectors.

use super::*;

fn rustc() -> PathBuf {
    PathBuf::from("/tool/rustc")
}

fn lld() -> PathBuf {
    PathBuf::from("/tool/ld.lld")
}

fn clang() -> PathBuf {
    PathBuf::from("/tool/clang")
}

#[test]
fn the_probe_table_is_p0_to_p9() {
    assert_eq!(
        probe_ids(),
        vec!["P0", "P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "P9"]
    );
}

#[test]
fn selection_parses_and_sorts() {
    assert_eq!(parse_selection("P3,P1").expect("two"), vec!["P1", "P3"]);
    assert_eq!(parse_selection("all").expect("all").len(), 10);
    assert!(parse_selection("P2m").is_err());
    assert!(parse_selection("").is_err());
}

#[test]
fn sources_follow_the_rd_new_layout() {
    let root = Path::new("/repo");
    assert_eq!(
        source_path(root, Lang::Rust, "P2"),
        PathBuf::from("/repo/tests/libc/size/probes/rust/p2.rs")
    );
    assert_eq!(
        source_path(root, Lang::C, "P8"),
        PathBuf::from("/repo/tests/libc/size/probes/c/p8.c")
    );
    assert_eq!(
        start_support_path(root),
        PathBuf::from("/repo/tests/libc/size/probes/rust/start_support.rs")
    );
}

#[test]
fn static_rust_recipe_has_the_four_flags_and_gc_sections() {
    let spec = rust_static_command(
        &rustc(),
        Path::new("/repo"),
        Arch::X86_64,
        Set::Tiny,
        "P2",
        Path::new("/repo/build/x86_64"),
        Path::new("/repo/build/size/x86_64/tiny/static/rust"),
        &[],
    );
    assert_eq!(spec.program(), &rustc());
    let args = spec.args().join(" ");
    for expected in [
        "panic=abort",
        "opt-level=z",
        "relocation-model=static",
        "link-arg=--gc-sections",
        "link-arg=-no-pie",
        "strip=none",
        "--target x86_64-unknown-none",
        "link-arg=/repo/build/x86_64/crt1.o",
        "link-arg=/repo/build/x86_64/libc.a",
    ] {
        assert!(args.contains(expected), "missing `{expected}` in {args}");
    }
    assert!(!args.contains("sh -c"));
}

#[test]
fn static_rust_recipe_uses_s_for_server() {
    let spec = rust_static_command(
        &rustc(),
        Path::new("/repo"),
        Arch::X86_64,
        Set::Server,
        "P2",
        Path::new("/libc"),
        Path::new("/out"),
        &[],
    );
    assert!(spec.args().join(" ").contains("opt-level=s"));
}

#[test]
fn dynamic_rust_recipe_has_the_pg53_flags() {
    let spec = rust_dynamic_command(
        &rustc(),
        Path::new("/repo"),
        Arch::Aarch64,
        "P1",
        Path::new("/libc"),
        Path::new("/out"),
        &[],
    );
    let args = spec.args().join(" ");
    assert!(args.contains("relocation-model=pic"));
    assert!(args.contains("link-arg=-pie"));
    assert!(args.contains("--dynamic-linker=/libexec/ld-elf.so.1"));
    assert!(args.contains("link-arg=max-page-size=4096"));
    assert!(args.contains("Scrt1.o"));
    assert!(args.contains("libc.so.1"));
    assert!(args.contains("--target aarch64-unknown-none-softfloat"));
}

#[test]
fn deterministic_argument_order() {
    let build = |_: ()| {
        rust_static_command(
            &rustc(),
            Path::new("/repo"),
            Arch::Riscv64,
            Set::Tiny,
            "P0",
            Path::new("/libc"),
            Path::new("/out"),
            &[],
        )
        .args()
        .to_vec()
    };
    assert_eq!(build(()), build(()));
}

#[test]
fn p9_adds_u_for_every_symbol() {
    let symbols = vec!["printf".to_string(), "exit".to_string()];
    let spec = rust_static_command(
        &rustc(),
        Path::new("/repo"),
        Arch::X86_64,
        Set::Server,
        "P9",
        Path::new("/libc"),
        Path::new("/out"),
        &symbols,
    );
    let args = spec.args();
    let has_pair = |symbol: &str| {
        args.windows(4).any(|pair| {
            pair[0] == "-C"
                && pair[1] == "link-arg=-u"
                && pair[2] == "-C"
                && pair[3] == format!("link-arg={symbol}")
        })
    };
    assert!(has_pair("printf"));
    assert!(has_pair("exit"));
    assert!(args.iter().any(|arg| arg.ends_with("p9_all.rs")));
}

#[test]
fn every_rust_recipe_carries_gc_sections_r4() {
    let root = Path::new("/repo");
    let mut vectors = Vec::new();
    for arch in Arch::all() {
        for set in [Set::Tiny, Set::Server] {
            vectors.push(
                rust_static_command(&rustc(), root, *arch, set, "P1", root, root, &[])
                    .args()
                    .join(" "),
            );
        }
        vectors.push(
            rust_dynamic_command(&rustc(), root, *arch, "P1", root, root, &[])
                .args()
                .join(" "),
        );
    }
    for vector in vectors {
        assert!(vector.contains("--gc-sections"), "{vector}");
    }
}

#[test]
fn c_compile_uses_the_sysroot_and_no_standard_headers() {
    let spec = c_compile_command(
        &clang(),
        Path::new("/repo"),
        Arch::X86_64,
        Set::Tiny,
        "P2",
        Path::new("/out"),
    );
    let args = spec.args().join(" ");
    assert!(args.contains("-target x86_64-unknown-none-elf"));
    assert!(args.contains("-ffreestanding"));
    assert!(args.contains("-nostdinc"));
    assert!(args.contains("/repo/libc/ravenlibc/include"));
    assert!(args.contains("-ffunction-sections"));
    assert!(args.contains("-fdata-sections"));
    assert!(!args.contains(" -std="), "plain C, no host libc");
}

#[test]
fn riscv_c_probe_uses_the_lp64d_abi() {
    let spec = c_compile_command(
        &clang(),
        Path::new("/repo"),
        Arch::Riscv64,
        Set::Tiny,
        "P0",
        Path::new("/out"),
    );
    let args = spec.args().join(" ");
    assert!(args.contains("-march=rv64gc"));
    assert!(args.contains("-mabi=lp64d"));
}

#[test]
fn x86_c_probe_has_no_riscv_flags() {
    let spec = c_compile_command(
        &clang(),
        Path::new("/repo"),
        Arch::X86_64,
        Set::Tiny,
        "P0",
        Path::new("/out"),
    );
    assert!(!spec.args().join(" ").contains("lp64"));
}

#[test]
fn c_link_orders_crt_probe_archive_crtn() {
    let spec = c_link_command(
        &lld(),
        Path::new("/repo"),
        Arch::X86_64,
        "P2",
        Path::new("/libc"),
        Path::new("/out"),
    );
    let args = spec.args();
    let positions: Vec<usize> = ["crt1.o", "crti.o", "p2.o", "libc.a", "crtn.o"]
        .iter()
        .map(|needle| {
            args.iter()
                .position(|arg| arg.ends_with(needle))
                .unwrap_or(usize::MAX)
        })
        .collect();
    for window in positions.windows(2) {
        assert!(window[0] < window[1], "bad order: {positions:?}");
    }
    assert!(args.iter().any(|arg| arg == "--gc-sections"));
}

#[test]
fn c_dynamic_link_uses_scrt1_and_pie() {
    let spec = c_link_dynamic_command(
        &lld(),
        Path::new("/repo"),
        Arch::Riscv64,
        "P1",
        Path::new("/libc"),
        Path::new("/out"),
    );
    let args = spec.args().join(" ");
    assert!(args.contains("Scrt1.o"));
    assert!(args.contains("-pie"));
    assert!(args.contains("--dynamic-linker=/libexec/ld-elf.so.1"));
    assert!(args.contains("max-page-size=4096"));
}

#[test]
fn p9_source_is_a_silent_no_std_main() {
    assert!(P9_SOURCE.contains("#![no_std]"));
    assert!(P9_SOURCE.contains("#![no_main]"));
    assert!(P9_SOURCE.contains("fn panic"));
    assert!(!P9_SOURCE.contains("format!"));
}
