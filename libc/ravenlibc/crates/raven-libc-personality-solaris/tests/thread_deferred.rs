// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! C57 is absent on this branch. This test is the machine-readable status report the ticket
//! asks for: it reads `exports.toml`, asserts that exactly the thread-runtime exports are marked
//! `deferred = "C57"`, and prints every one of them.

use std::path::PathBuf;

fn exports() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("exports.toml")
}

struct Export {
    name: String,
    deferred: Option<String>,
}

fn parse() -> Vec<Export> {
    let text = std::fs::read_to_string(exports()).expect("exports.toml");
    let mut rows = Vec::new();
    let mut current: Option<Export> = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "[[export]]" {
            if let Some(row) = current.take() {
                rows.push(row);
            }
            current = Some(Export {
                name: String::new(),
                deferred: None,
            });
        } else if let Some(row) = current.as_mut() {
            if let Some(value) = line.strip_prefix("name = \"") {
                row.name = value.trim_end_matches('"').to_string();
            } else if let Some(value) = line.strip_prefix("deferred = \"") {
                row.deferred = Some(value.trim_end_matches('"').to_string());
            }
        }
    }
    if let Some(row) = current {
        rows.push(row);
    }
    rows
}

/// The 32 exports whose body needs `raven-libc-thread` (one per pthread/semaphore call).
const C57_SYMBOLS: &[&str] = &[
    "thr_create",
    "thr_join",
    "thr_kill",
    "thr_sigsetmask",
    "thr_keycreate",
    "thr_setspecific",
    "thr_getspecific",
    "thr_getprio",
    "thr_setprio",
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

#[test]
fn the_export_table_has_fifty_names() {
    assert_eq!(parse().len(), 50);
}

#[test]
fn the_c57_deferred_list_is_exact() {
    let rows = parse();
    let deferred: Vec<&str> = rows
        .iter()
        .filter(|row| row.deferred.as_deref() == Some("C57"))
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(deferred.len(), C57_SYMBOLS.len());
    for name in C57_SYMBOLS {
        assert!(deferred.contains(name), "missing DEFERRED(C57) {name}");
    }
    for name in &deferred {
        assert!(
            C57_SYMBOLS.contains(name),
            "unexpected deferred symbol {name}"
        );
    }
}

#[test]
fn the_c57_list_is_reported_with_the_exact_symbols() {
    let rows = parse();
    let mut reported = 0usize;
    for row in &rows {
        if row.deferred.as_deref() == Some("C57") {
            println!("DEFERRED(C57): {}", row.name);
            reported += 1;
        }
    }
    println!("DEFERRED(C57): {reported} symbols");
    assert_eq!(reported, C57_SYMBOLS.len());
}

#[test]
fn the_independent_exports_are_not_deferred() {
    let rows = parse();
    for name in [
        "port_create",
        "port_associate",
        "port_dissociate",
        "port_get",
        "port_getn",
        "port_send",
        "port_alert",
        "getexecname",
        "getisax",
        "thr_min_stack",
        "thr_main",
        "thr_self",
        "thr_yield",
        "thr_setconcurrency",
        "thr_getconcurrency",
        "thr_exit",
        "thr_suspend",
        "thr_continue",
    ] {
        let row = rows
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(row.deferred, None, "{name} must not be DEFERRED(C57)");
    }
}

#[test]
fn every_deferred_name_is_a_thread_runtime_name() {
    for row in parse() {
        if row.deferred.as_deref() == Some("C57") {
            assert!(
                row.name.starts_with("thr_")
                    || row.name.starts_with("mutex_")
                    || row.name.starts_with("cond_")
                    || row.name.starts_with("rw")
                    || row.name.starts_with("sema_"),
                "{} is not a thread-runtime export",
                row.name
            );
        }
    }
}
