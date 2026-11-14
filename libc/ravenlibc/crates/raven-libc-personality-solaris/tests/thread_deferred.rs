// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `raven-libc-thread` is merged: this test is the machine-readable status report that the
//! thread-runtime exports are no longer deferred and that the 50-name table is complete.

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

/// The 32 exports that need `raven-libc-thread` (one per pthread/semaphore call).
const THREAD_RUNTIME_SYMBOLS: &[&str] = &[
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
fn no_export_is_deferred() {
    for row in parse() {
        assert_eq!(
            row.deferred, None,
            "{} is still deferred after the C57 merge",
            row.name
        );
    }
}

#[test]
fn the_thread_runtime_exports_are_present() {
    let rows = parse();
    for name in THREAD_RUNTIME_SYMBOLS {
        assert!(
            rows.iter().any(|row| row.name == *name),
            "missing thread-runtime export {name}"
        );
    }
}

#[test]
fn the_independent_exports_are_present() {
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
        assert!(
            rows.iter().any(|row| row.name == name),
            "missing independent export {name}"
        );
    }
}
