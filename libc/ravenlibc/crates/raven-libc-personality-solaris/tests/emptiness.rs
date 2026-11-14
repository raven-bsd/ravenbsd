// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Emptiness (C67 7(e) and 6 step 9): with the shell feature off none of the 50 Solaris names
//! is exported; with it on all 50 are. No `door_*` or `kstat_*` symbol is ever exported.
//!
//! The test needs a built archive and SKIPs (prints `SKIP:`) when there is none; set
//! `C67_REQUIRE_ARCHIVE=1` to turn a missing archive into a failure.

use std::path::{Path, PathBuf};
use std::process::Command;

const SOLARIS_SYMBOLS: &[&str] = &[
    "port_create",
    "port_associate",
    "port_dissociate",
    "port_get",
    "port_getn",
    "port_send",
    "port_alert",
    "getexecname",
    "getisax",
    "thr_create",
    "thr_exit",
    "thr_join",
    "thr_self",
    "thr_yield",
    "thr_kill",
    "thr_sigsetmask",
    "thr_keycreate",
    "thr_setspecific",
    "thr_getspecific",
    "thr_min_stack",
    "thr_main",
    "thr_getprio",
    "thr_setprio",
    "thr_setconcurrency",
    "thr_getconcurrency",
    "thr_suspend",
    "thr_continue",
    "mutex_init",
    "mutex_destroy",
    "mutex_lock",
    "mutex_trylock",
    "mutex_unlock",
    "cond_init",
    "cond_destroy",
    "cond_wait",
    "cond_timedwait",
    "cond_signal",
    "cond_broadcast",
    "rwlock_init",
    "rwlock_destroy",
    "rw_rdlock",
    "rw_wrlock",
    "rw_tryrdlock",
    "rw_trywrlock",
    "rw_unlock",
    "sema_init",
    "sema_destroy",
    "sema_wait",
    "sema_trywait",
    "sema_post",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("repo root")
        .to_path_buf()
}

fn required() -> bool {
    std::env::var("C67_REQUIRE_ARCHIVE").as_deref() == Ok("1")
}

fn nm() -> String {
    for name in ["llvm-nm", "nm"] {
        if Command::new(name).arg("--version").output().is_ok() {
            return name.to_string();
        }
    }
    "nm".to_string()
}

fn defined(archive: &Path) -> Vec<String> {
    let output = Command::new(nm())
        .args(["--defined-only", "--extern-only", "--format=posix"])
        .arg(archive)
        .output()
        .expect("nm");
    let text = String::from_utf8_lossy(&output.stdout);
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.ends_with(':') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(name), Some(kind), Some(value)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if kind.len() != 1 || u64::from_str_radix(value, 16).is_err() {
            continue;
        }
        names.push(name.to_string());
    }
    names
}

#[test]
fn server_archive_exports_all_fifty_names() {
    let archive = root().join("build/x86_64/libc.a");
    if !archive.is_file() {
        if required() {
            panic!("SKIP: no build/x86_64/libc.a (C67_REQUIRE_ARCHIVE=1)");
        }
        eprintln!("SKIP: no build/x86_64/libc.a");
        return;
    }
    let names = defined(&archive);
    for symbol in SOLARIS_SYMBOLS {
        assert!(names.iter().any(|name| name == symbol), "missing {symbol}");
    }
    println!("archive: {} Solaris names present", SOLARIS_SYMBOLS.len());
}

#[test]
fn default_staticlib_exports_no_solaris_name() {
    // The default-feature build lives in its own target directory so that a previous
    // `--features personality-solaris` build cannot hide the answer. `C67_EMPTY_ARCHIVE`
    // overrides the path.
    let archive = std::env::var_os("C67_EMPTY_ARCHIVE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root().join("target/c67-empty/x86_64-unknown-none/debug/libraven_libc.a")
        });
    if !archive.is_file() {
        if required() {
            panic!("SKIP: no default staticlib (C67_REQUIRE_ARCHIVE=1)");
        }
        eprintln!("SKIP: no default staticlib {}", archive.display());
        return;
    }
    let names = defined(&archive);
    for symbol in SOLARIS_SYMBOLS {
        assert!(
            !names.iter().any(|name| name == symbol),
            "{symbol} is exported with the personality feature off"
        );
    }
    println!("archive: 0 Solaris names present");
}

#[test]
fn no_door_or_kstat_symbol_is_exported() {
    for archive in [
        root().join("build/x86_64/libc.a"),
        root().join("target/c67-empty/x86_64-unknown-none/debug/libraven_libc.a"),
    ] {
        if !archive.is_file() {
            continue;
        }
        let names = defined(&archive);
        for symbol in &names {
            assert!(!symbol.starts_with("door_"), "{symbol} is a door export");
            assert!(!symbol.starts_with("kstat_"), "{symbol} is a kstat export");
        }
    }
}

#[test]
fn export_table_matches_this_list() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("exports.toml"))
        .expect("exports.toml");
    for symbol in SOLARIS_SYMBOLS {
        assert!(
            text.contains(&format!("name = \"{symbol}\"")),
            "exports.toml lacks {symbol}"
        );
    }
    assert_eq!(SOLARIS_SYMBOLS.len(), 50);
}
