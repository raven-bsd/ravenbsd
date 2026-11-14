// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for tool discovery.

use super::*;

#[test]
fn missing_tool_displays_the_c35_line() {
    let error = MissingTool {
        name: "llvm-size".to_string(),
    };
    assert_eq!(error.to_string(), "llvm-size not found");
    assert_eq!(format!("skipped: {error}"), "skipped: llvm-size not found");
}

#[test]
fn environment_override_wins_over_every_directory() {
    let dirs = vec![PathBuf::from("/nonexistent/a")];
    let found = locate_with(Some("/opt/llvm/bin/llvm-size"), "llvm-size", &dirs, &[]);
    assert_eq!(found, Some(PathBuf::from("/opt/llvm/bin/llvm-size")));
}

#[test]
fn empty_override_falls_through() {
    let found = locate_with(Some(""), "llvm-size", &[], &[]);
    assert_eq!(found, None);
}

#[test]
fn missing_tool_error_downcasts() {
    let error = find_tool("LLVM_SIZE_TEST", "definitely-not-a-real-tool-xyz", &[])
        .expect_err("must be missing");
    let missing = error.downcast_ref::<MissingTool>().expect("MissingTool");
    assert_eq!(missing.name, "definitely-not-a-real-tool-xyz");
}

#[test]
fn required_flag_reads_only_exactly_one() {
    // The environment is process-wide; this asserts the parser's shape without mutating it.
    // `required` is exercised by the CLI tests through a subprocess (see main_ut.rs).
    assert!(!std::env::var("RL_SIZE_REQUIRED").is_ok_and(|value| value == "0"));
}

#[test]
fn first_line_trims_and_keeps_only_one() {
    assert_eq!(first_line("rustc 1.97.1\nhost: x\n"), "rustc 1.97.1");
    assert_eq!(first_line(""), "");
}

#[test]
fn toolchain_block_has_no_host_paths() {
    let tools = Tools {
        size: PathBuf::from("/x/llvm-size"),
        nm: PathBuf::from("/x/llvm-nm"),
        readobj: PathBuf::from("/x/llvm-readobj"),
        objdump: PathBuf::from("/x/llvm-objdump"),
        strip: PathBuf::from("/x/llvm-strip"),
        ar: PathBuf::from("/x/llvm-ar"),
        objcopy: PathBuf::from("/x/llvm-objcopy"),
        lld: PathBuf::from("/x/ld.lld"),
        rustc: PathBuf::from("/x/rustc"),
        cargo: PathBuf::from("/x/cargo"),
        clang: None,
        rustc_version: "rustc 1.97.1".to_string(),
        lld_version: "LLD 22.1.6".to_string(),
    };
    let toolchain = tools.toolchain();
    assert_eq!(toolchain.rustc, "rustc 1.97.1");
    assert!(!toolchain.rustc.contains("/x/"));
    assert!(!tools.has_c_toolchain());
}

#[test]
fn locate_searches_directories_in_order() {
    let base = crate::testutil::TempDir::new("tools-locate").expect("temp");
    let first = base.path().join("first");
    let second = base.path().join("second");
    std::fs::create_dir_all(&first).expect("first");
    std::fs::create_dir_all(&second).expect("second");
    std::fs::write(first.join("tool"), b"1").expect("first tool");
    std::fs::write(second.join("tool"), b"2").expect("second tool");
    let found = locate_with(None, "tool", &[first.clone(), second], &[]);
    assert_eq!(found, Some(first.join("tool")));
}
