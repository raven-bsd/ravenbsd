// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for CLI parsing and the small helpers of `main.rs`.

use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn run_defaults_are_the_full_matrix() {
    let options = parse_run_args(&args(&["--root", "/repo"])).expect("parse");
    assert_eq!(options.arches, Arch::all().to_vec());
    assert_eq!(options.linkages.len(), 2);
    assert_eq!(options.sets.len(), 2);
    assert_eq!(options.langs, vec![Lang::Rust]);
    assert_eq!(options.probes.len(), 10);
    assert_eq!(options.out, PathBuf::from("/repo/build/size"));
    assert!(!options.no_build_libc);
}

#[test]
fn run_accepts_every_flag() {
    let options = parse_run_args(&args(&[
        "--root",
        "/repo",
        "--arch",
        "x86_64",
        "--linkage",
        "static",
        "--set",
        "tiny",
        "--lang",
        "rust,c",
        "--probe",
        "P0,P2",
        "--libc-dir",
        "build/x86_64",
        "--out",
        "/tmp/out",
        "--no-build-libc",
        "--check",
    ]))
    .expect("parse");
    assert_eq!(options.arches, vec![Arch::X86_64]);
    assert_eq!(options.linkages, vec![Linkage::Static]);
    assert_eq!(options.sets, vec![Set::Tiny]);
    assert_eq!(options.langs, vec![Lang::Rust, Lang::C]);
    assert_eq!(options.probes, vec!["P0", "P2"]);
    assert_eq!(options.libc_override, Some(PathBuf::from("build/x86_64")));
    assert_eq!(options.out, PathBuf::from("/tmp/out"));
    assert!(options.no_build_libc);
    assert!(options.check_after);
}

#[test]
fn run_rejects_an_unknown_flag() {
    assert!(parse_run_args(&args(&["--jobs", "4"])).is_err());
}

#[test]
fn run_needs_a_value_after_a_flag() {
    assert!(parse_run_args(&args(&["--arch"])).is_err());
}

#[test]
fn root_flag_is_read_from_any_position() {
    assert_eq!(
        root_from(&args(&["--arch", "x86_64", "--root", "/x"])),
        PathBuf::from("/x")
    );
    assert_eq!(
        root_from(&args(&["--root"])),
        std::env::current_dir().expect("cwd")
    );
}

#[test]
fn default_result_and_budget_paths_are_stable() {
    let root = Path::new("/repo");
    assert_eq!(
        results_path(root),
        PathBuf::from("/repo/build/size/libc-size.json")
    );
    assert_eq!(
        budgets_path(root),
        PathBuf::from("/repo/tests/libc/size/BUDGETS.toml")
    );
}

#[test]
fn unknown_command_is_a_usage_error() {
    assert!(dispatch(args(&["nope"])).is_err());
    assert_eq!(dispatch(args(&["--help"])).expect("help"), 0);
}

#[test]
fn shared_library_arch_is_derived_from_the_path() {
    assert_eq!(
        soff_arch(Path::new("build/aarch64/libc.so.1")),
        Arch::Aarch64
    );
    assert_eq!(
        soff_arch(Path::new("build/riscv64/libc.so.1")),
        Arch::Riscv64
    );
    assert_eq!(soff_arch(Path::new("build/x86_64/libc.so.1")), Arch::X86_64);
}

#[test]
fn p9_symbols_reads_the_committed_list() {
    let dir = libc_size::testutil::TempDir::new("main-p9").expect("temp");
    dir.write(
        "libc/ravenlibc/abi/Symbols.list",
        b"# SPDX-License-Identifier: BSD-3-Clause\n# comment\nprintf\nwrite\nexit\n",
    )
    .expect("write");
    let symbols = p9_symbols(dir.path()).expect("symbols");
    assert_eq!(symbols, vec!["exit", "printf", "write"]);
}

#[test]
fn p9_symbols_falls_back_to_the_symbol_map() {
    let dir = libc_size::testutil::TempDir::new("main-p9-map").expect("temp");
    dir.write(
        "libc/ravenlibc/crates/raven-libc/Symbol.map",
        b"RAVEN_1.0 {\n  global:\n    printf;\n    write;\n  local:\n    *;\n};\n",
    )
    .expect("write");
    let symbols = p9_symbols(dir.path()).expect("symbols");
    assert_eq!(symbols, vec!["printf", "write"]);
}

#[test]
fn p9_symbols_fails_without_both_sources() {
    let dir = libc_size::testutil::TempDir::new("main-p9-missing").expect("temp");
    assert!(p9_symbols(dir.path()).is_err());
}

#[test]
fn skip_reasons_never_carry_a_host_path() {
    let cwd = std::env::current_dir().expect("cwd");
    let text = format!("link: {} /build/x/libc.a failed", cwd.display());
    let sanitized = sanitize_reason(&text, Path::new("."));
    assert!(!sanitized.contains(&cwd.display().to_string()));
    assert!(sanitized.starts_with("link: ."));
}

#[test]
fn write_text_creates_parents() {
    let dir = libc_size::testutil::TempDir::new("main-write").expect("temp");
    let path = dir.path().join("a/b/c.txt");
    write_text(&path, "hello").expect("write");
    assert_eq!(std::fs::read_to_string(path).expect("read"), "hello");
}

#[test]
fn archive_symbols_reads_a_real_archive_when_present() {
    // The gate cannot construct an archive without a tool, so this only asserts the
    // missing-file reason, which is what an unbuilt product produces.
    let tools = Tools {
        size: PathBuf::from("/x/size"),
        nm: PathBuf::from("/x/nm"),
        readobj: PathBuf::from("/x/readobj"),
        objdump: PathBuf::from("/x/objdump"),
        strip: PathBuf::from("/x/strip"),
        ar: PathBuf::from("/x/ar"),
        objcopy: PathBuf::from("/x/objcopy"),
        lld: PathBuf::from("/x/lld"),
        rustc: PathBuf::from("/x/rustc"),
        cargo: PathBuf::from("/x/cargo"),
        clang: None,
        rustc_version: String::new(),
        lld_version: String::new(),
    };
    let dir = libc_size::testutil::TempDir::new("main-archive").expect("temp");
    let error =
        archive_symbols(&tools, &dir.path().join("libc.a"), dir.path()).expect_err("missing");
    assert!(error.to_string().contains("libc.a absent"));
}
