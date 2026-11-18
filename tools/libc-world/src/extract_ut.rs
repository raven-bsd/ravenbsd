// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;
use crate::testutil::{TempDir, require, write};

fn compile(temp: &TempDir, name: &str, source: &str, target: &str) -> Option<PathBuf> {
    if !require(&["clang"]) {
        return None;
    }
    let source_path = temp.join(&format!("{name}.c"));
    write(&source_path, source);
    let object = temp.join(&format!("{name}.o"));
    let status = Command::new("clang")
        .args([
            &format!("--target={target}"),
            "-ffreestanding",
            "-c",
            source_path.to_str().expect("path"),
            "-o",
            object.to_str().expect("path"),
        ])
        .status()
        .expect("clang");
    assert!(status.success(), "clang failed for {name}");
    Some(object)
}

fn nm() -> &'static str {
    if crate::testutil::tool_available("llvm-nm") {
        "llvm-nm"
    } else {
        "nm"
    }
}

#[test]
fn parse_deps_matches_the_ticket_sample() {
    let text = "a.o: a.c /w/sysroot/usr/include/stdio.h \\\n /w/sysroot/usr/include/sys/cdefs.h /w/sysroot/usr/include/bits/alltypes.h \\\n ../local.h /w/sysroot/usr/include/stdio.h\n";
    let headers = parse_deps(text, Path::new("/w/sysroot"));
    assert_eq!(
        headers.into_iter().collect::<Vec<_>>(),
        vec!["bits/alltypes.h", "stdio.h", "sys/cdefs.h"]
    );
}

#[test]
fn parse_deps_handles_escaped_spaces() {
    let text = "a.o: /w/sysroot/usr/include/foo\\ bar.h /w/sysroot/usr/include/baz.h\n";
    let headers = parse_deps(text, Path::new("/w/sysroot"));
    assert!(headers.contains("foo bar.h"));
    assert!(headers.contains("baz.h"));
}

#[test]
fn parse_deps_ignores_host_paths() {
    let text = "a.o: /usr/include/stdio.h /w/sysroot/usr/include/raven.h\n";
    let headers = parse_deps(text, Path::new("/w/sysroot"));
    assert_eq!(headers.into_iter().collect::<Vec<_>>(), vec!["raven.h"]);
}

#[test]
fn parse_deps_ignores_rules_without_a_colon() {
    assert!(parse_deps("plain text\n", Path::new("/w")).is_empty());
}

#[test]
fn parse_calls_takes_both_l_forms() {
    let text = "link -o prog a.o b.o -lm -lpthread -L/x -l z\n";
    let libs = parse_calls(text);
    assert_eq!(
        libs.into_iter().collect::<Vec<_>>(),
        vec!["m", "pthread", "z"]
    );
}

#[test]
fn parse_calls_ignores_compile_lines() {
    let text = "compile -c a.c -o a.o -lm\n";
    assert!(parse_calls(text).is_empty());
}

#[test]
fn parse_calls_ignores_library_search_paths() {
    let text = "link -L/usr/lib -Wl,-rpath,/x -lc\n";
    let libs = parse_calls(text);
    assert_eq!(libs.into_iter().collect::<Vec<_>>(), vec!["c"]);
}

#[test]
fn parse_calls_deduplicates() {
    let text = "link -lm\nlink -lm\n";
    assert_eq!(parse_calls(text).len(), 1);
}

#[test]
fn undef_text_is_sorted_and_tab_separated() {
    let mut map = BTreeMap::new();
    map.insert("zebra".to_string(), 'U');
    map.insert("alpha".to_string(), 'w');
    assert_eq!(undef_text(&map), "alpha\tw\nzebra\tU\n");
}

#[test]
fn an_x86_64_object_has_the_target_machine() {
    let temp = TempDir::new("extract-elf");
    let Some(object) = compile(&temp, "a", "void f(void) {}\n", "x86_64-unknown-ravenbsd") else {
        return;
    };
    assert!(target_object(&object, 0x3e).expect("target"));
}

#[test]
fn an_aarch64_object_is_ignored_by_an_x86_64_scan() {
    let temp = TempDir::new("extract-arch");
    let Some(object) = compile(&temp, "a", "void f(void) {}\n", "aarch64-unknown-ravenbsd") else {
        return;
    };
    assert!(!target_object(&object, 0x3e).expect("target"));
    assert!(target_object(&object, 0xb7).expect("target"));
}

#[test]
fn a_text_file_is_not_an_object() {
    let temp = TempDir::new("extract-text");
    let path = temp.join("a.txt");
    write(&path, "hello\n");
    assert!(!target_object(&path, 0x3e).expect("target"));
}

#[test]
fn undefined_and_defined_symbols_match_the_golden() {
    let temp = TempDir::new("extract-undef");
    let Some(first) = compile(
        &temp,
        "a",
        "void helper(void) {}\nextern void strlcpy(void);\nvoid f(void) { strlcpy(); }\n",
        "x86_64-unknown-ravenbsd",
    ) else {
        return;
    };
    let Some(second) = compile(
        &temp,
        "b",
        "extern void helper(void);\nvoid g(void) { helper(); }\nextern void __cxa_finalize(void *) __attribute__((weak));\nvoid h(void) { if (__cxa_finalize) __cxa_finalize(0); }\n",
        "x86_64-unknown-ravenbsd",
    ) else {
        return;
    };
    let extraction = scan(
        nm(),
        &[temp.path().to_path_buf()],
        "x86_64",
        Path::new("/w/sysroot"),
        &temp.join("log"),
    )
    .expect("scan");
    assert_eq!(extraction.undefined.get("strlcpy"), Some(&'U'));
    assert_eq!(extraction.undefined.get("__cxa_finalize"), Some(&'w'));
    assert!(!extraction.undefined.contains_key("helper"));
    assert_eq!(extraction.undefined.len(), 2);
    assert_eq!(
        undef_text(&extraction.undefined),
        "__cxa_finalize\tw\nstrlcpy\tU\n"
    );
    assert!(first.is_file() && second.is_file());
}

#[test]
fn an_archive_is_scanned() {
    if !require(&["llvm-ar"]) {
        return;
    }
    let temp = TempDir::new("extract-ar");
    let Some(object) = compile(
        &temp,
        "a",
        "extern void strlcpy(void);\nvoid f(void) { strlcpy(); }\n",
        "x86_64-unknown-ravenbsd",
    ) else {
        return;
    };
    let archive = temp.join("liba.a");
    let status = Command::new("llvm-ar")
        .arg("rcs")
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("ar");
    assert!(status.success());
    let extraction = scan(
        nm(),
        &[temp.path().to_path_buf()],
        "x86_64",
        Path::new("/w/sysroot"),
        &temp.join("log"),
    )
    .expect("scan");
    assert!(extraction.undefined.contains_key("strlcpy"));
}

#[test]
fn an_archive_with_a_foreign_member_is_ignored() {
    if !require(&["llvm-ar"]) {
        return;
    }
    let temp = TempDir::new("extract-ar-foreign");
    let Some(object) = compile(
        &temp,
        "a",
        "extern void strlcpy(void);\nvoid f(void) { strlcpy(); }\n",
        "aarch64-unknown-ravenbsd",
    ) else {
        return;
    };
    let archive = temp.join("liba.a");
    let status = Command::new("llvm-ar")
        .arg("rcs")
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("ar");
    assert!(status.success());
    assert!(!target_object(&archive, 0x3e).expect("target"));
}

#[test]
fn internal_names_are_dropped() {
    let temp = TempDir::new("extract-drop");
    let Some(object) = compile(
        &temp,
        "a",
        "extern void _end(void);\nextern void __gnu_foo(void);\nvoid f(void) { _end(); __gnu_foo(); }\n",
        "x86_64-unknown-ravenbsd",
    ) else {
        return;
    };
    let extraction = scan(
        nm(),
        &[temp.path().to_path_buf()],
        "x86_64",
        Path::new("/w/sysroot"),
        &temp.join("log"),
    )
    .expect("scan");
    assert!(!extraction.undefined.contains_key("_end"));
    assert!(!extraction.undefined.contains_key("__gnu_foo"));
    assert!(object.is_file());
}

#[test]
fn a_missing_directory_yields_an_empty_extraction() {
    let temp = TempDir::new("extract-empty");
    let extraction = scan(
        nm(),
        &[temp.join("nope")],
        "x86_64",
        Path::new("/w/sysroot"),
        &temp.join("log"),
    )
    .expect("scan");
    assert_eq!(extraction, Extraction::default());
}

#[test]
fn a_bad_nm_is_an_error_when_an_object_exists() {
    let temp = TempDir::new("extract-bad-nm");
    let Some(_object) = compile(&temp, "a", "void f(void) {}\n", "x86_64-unknown-ravenbsd") else {
        return;
    };
    assert!(
        scan(
            "definitely-not-nm-xyz",
            &[temp.path().to_path_buf()],
            "x86_64",
            Path::new("/w/sysroot"),
            &temp.join("log"),
        )
        .is_err()
    );
}

#[test]
fn dependency_files_are_read_from_the_log_directory() {
    let temp = TempDir::new("extract-deps");
    let deps = temp.join("log/deps");
    fs::create_dir_all(&deps).expect("deps");
    write(
        &deps.join("1.d"),
        "a.o: a.c /w/sysroot/usr/include/stdio.h /w/sysroot/usr/include/sys/cdefs.h\n",
    );
    let Some(_object) = compile(&temp, "a", "void f(void) {}\n", "x86_64-unknown-ravenbsd") else {
        return;
    };
    let extraction = scan(
        nm(),
        &[temp.path().to_path_buf()],
        "x86_64",
        Path::new("/w/sysroot"),
        &temp.join("log"),
    )
    .expect("scan");
    assert!(extraction.headers.contains("stdio.h"));
    assert!(extraction.headers.contains("sys/cdefs.h"));
    assert_eq!(extraction.headers.len(), 2);
}

#[test]
fn link_libraries_are_read_from_calls_log() {
    let temp = TempDir::new("extract-libs");
    write(&temp.join("log/calls.log"), "link -o p a.o -lm -lz\n");
    let extraction = scan(
        nm(),
        &[temp.join("nope")],
        "x86_64",
        Path::new("/w/sysroot"),
        &temp.join("log"),
    )
    .expect("scan");
    assert_eq!(
        extraction.libs.into_iter().collect::<Vec<_>>(),
        vec!["m", "z"]
    );
}
