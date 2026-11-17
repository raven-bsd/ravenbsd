// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::Path;

use super::*;
use libc_world::builders::PlannedStep;
use libc_world::testutil::{TempDir, require, tool_available, write};

#[test]
fn a_value_parser_reads_a_flag_and_its_value() {
    let args = vec!["--arch".to_string(), "x86_64".to_string()];
    let mut parser = ValueParser::new(&args);
    assert_eq!(parser.next(), Some("--arch"));
    assert_eq!(parser.value("--arch").expect("value"), "x86_64");
    assert_eq!(parser.next(), None);
}

#[test]
fn a_missing_value_is_an_error() {
    let args = vec!["--arch".to_string()];
    let mut parser = ValueParser::new(&args);
    assert_eq!(parser.next(), Some("--arch"));
    assert!(parser.value("--arch").is_err());
}

#[test]
fn a_flag_is_not_accepted_as_a_value() {
    let args = vec!["--arch".to_string(), "--all".to_string()];
    let mut parser = ValueParser::new(&args);
    assert_eq!(parser.next(), Some("--arch"));
    assert!(parser.value("--arch").is_err());
}

#[test]
fn resolve_keeps_absolute_paths() {
    assert_eq!(
        resolve(Path::new("/root"), "/abs/path"),
        Path::new("/abs/path")
    );
}

#[test]
fn resolve_joins_relative_paths() {
    assert_eq!(
        resolve(Path::new("/root"), "build/x"),
        Path::new("/root/build/x")
    );
}

#[test]
fn resources_are_found_in_an_ancestor() {
    let temp = TempDir::new("main-resources");
    let resources = temp.join("ravenbsd-resources");
    fs::create_dir_all(&resources).expect("resources");
    let deep = temp.join("a/b/c");
    fs::create_dir_all(&deep).expect("deep");
    assert_eq!(discover_resources(&deep), resources);
}

#[test]
fn find_tool_finds_sh() {
    assert!(find_tool(&std::env::var("PATH").unwrap_or_default(), "sh").is_some());
}

#[test]
fn find_tool_returns_none_for_an_absent_name() {
    assert!(find_tool("/usr/bin:/bin", "definitely-not-a-tool-xyz").is_none());
}

#[test]
fn find_tool_accepts_an_absolute_path() {
    let path = find_tool("/usr/bin:/bin", "sh").expect("sh");
    assert_eq!(find_tool("/usr/bin:/bin", &path), Some(path));
}

#[test]
fn directory_size_sums_regular_files() {
    let temp = TempDir::new("main-size");
    write(&temp.join("a.txt"), "12345");
    write(&temp.join("sub/b.txt"), "678");
    assert_eq!(directory_size(temp.path()).expect("size"), 8);
}

#[test]
fn directory_size_is_zero_for_a_missing_directory() {
    let temp = TempDir::new("main-size-missing");
    assert_eq!(directory_size(&temp.join("nope")).expect("size"), 0);
}

fn options(temp: &TempDir) -> BuildOptions {
    BuildOptions {
        arch: "x86_64".to_string(),
        sysroot: temp.join("sysroot"),
        tarballs: temp.join("tarballs"),
        out: temp.join("out"),
        results: temp.join("results"),
        symbols: temp.join("symbols.toml"),
        jobs: 1,
        timeout_secs: 5,
        link: LinkMode::Static,
        host_alias: HostAlias::Raven,
        keep: false,
    }
}

fn tools(temp: &TempDir) -> Tools {
    let _ = temp;
    Tools {
        clang: "clang".to_string(),
        ld_lld: Some("ld.lld".to_string()),
        ar: "llvm-ar".to_string(),
        ranlib: "llvm-ranlib".to_string(),
        nm: "llvm-nm".to_string(),
        strip: "llvm-strip".to_string(),
        tar: "tar".to_string(),
        patch: "patch".to_string(),
        make: "make".to_string(),
        host_cc: "cc".to_string(),
        cmake: Some("cmake".to_string()),
        ninja: Some("ninja".to_string()),
        path: std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
    }
}

fn package() -> Package {
    Package {
        n: 25,
        name: "zlib".to_string(),
        tier: 1,
        build: "cf".to_string(),
        version: "1.0".to_string(),
        url: "x".to_string(),
        sha256: "0".repeat(64),
        tarball: "zlib.tar.gz".to_string(),
        needs: Vec::new(),
        optional: false,
        stresses: "stdio".to_string(),
        configure: Vec::new(),
        flag_notes: std::collections::BTreeMap::new(),
        make_args: Vec::new(),
        jobs_make: 1,
        tests: "none".to_string(),
        install: false,
        config_target: "defconfig".to_string(),
        config_lines: Vec::new(),
    }
}

#[test]
fn the_step_environment_has_the_fixed_variables() {
    let temp = TempDir::new("main-env");
    let options = options(&temp);
    let ctx = BuildContext {
        arch: "x86_64".to_string(),
        triple: "x86_64-unknown-ravenbsd".to_string(),
        build_triple: "x86_64-pc-linux-gnu".to_string(),
        wrapper: temp.join("bin/raven-cc"),
        ar: "llvm-ar".to_string(),
        ranlib: "llvm-ranlib".to_string(),
        make: "make".to_string(),
        host_cc: "cc".to_string(),
        sysroot: temp.join("sysroot"),
        toolchain_cmake: temp.join("toolchain.cmake"),
        host_alias: HostAlias::Raven,
    };
    let env = step_environment(
        &package(),
        &options,
        &tools(&temp),
        &ctx,
        &temp.join("log"),
        &temp.join("dest"),
    );
    let map: std::collections::BTreeMap<&str, &str> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    assert_eq!(map["CC"], ctx.wrapper.to_str().expect("path"));
    assert_eq!(map["TZ"], "UTC");
    assert_eq!(map["LC_ALL"], "C");
    assert!(map["PKG_CONFIG_LIBDIR"].ends_with("usr/lib/pkgconfig"));
    assert_eq!(map["CFLAGS"], "");
}

#[test]
fn execute_step_records_the_log() {
    if !require(&["sh"]) {
        return;
    }
    let temp = TempDir::new("main-run");
    let planned = PlannedStep {
        name: "make".to_string(),
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "echo hello".to_string()],
        cwd: temp.path().to_path_buf(),
        env: Vec::new(),
        stdin_null: true,
    };
    let env = vec![(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_default(),
    )];
    let output = execute_step(&planned, &env, 5, temp.path()).expect("step");
    assert_eq!(output.exit, Some(0));
    assert!(output.text.contains("hello"));
}

#[test]
fn tools_missing_reports_ld_lld() {
    let temp = TempDir::new("main-tools");
    let mut value = tools(&temp);
    value.ld_lld = None;
    assert_eq!(value.missing(false).as_deref(), Some("ld.lld"));
}

#[test]
fn tools_missing_checks_cmake_only_when_needed() {
    let temp = TempDir::new("main-tools-cmake");
    let mut value = tools(&temp);
    value.cmake = None;
    assert!(!value.missing(false).is_some_and(|tool| tool == "cmake"));
    assert_eq!(value.missing(true).as_deref(), Some("cmake"));
}

#[test]
fn tool_available_is_used_by_the_fixture_helpers() {
    assert!(tool_available("sh"));
}
