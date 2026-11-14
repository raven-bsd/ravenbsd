// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The async-signal-safe data file, the `const fn` lookup and the "no unsafe call on the entry
//! path" grep (ticket 4.9). `VERIFIED`/`UNVERIFIED` marks come from the local
//! `man-pages-5.10 signal-safety(7)` and the POSIX.1-2024 additions named by the ticket.

#![allow(
    unused_assignments,
    clippy::undocumented_unsafe_blocks,
    clippy::explicit_auto_deref,
    clippy::never_loop,
    clippy::identity_op,
    clippy::unnecessary_cast
)]
mod common;

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::fake::HostSigKernel;
use raven_libc_abi::structs::{SiginfoT, Sigval};
use raven_libc_signal::dispositions::{DispTable, Disposition, SA_SIGINFO};
use raven_libc_signal::{SigSet, is_async_signal_safe, mask, names, str2sig};

const SAFE_TEXT: &str = include_str!("../data/async-signal-safe.txt");
const NOT_SAFE_TEXT: &str = include_str!("../data/not-async-signal-safe.txt");
const ENTRY: &str = include_str!("../src/entry.rs");
const DELIVER: &str = include_str!("../src/deliver.rs");
const CRITICAL: &str = include_str!("../src/critical.rs");
const MASK_SRC: &str = include_str!("../src/mask.rs");

static LOCK: Mutex<()> = Mutex::new(());
static CALLS: AtomicUsize = AtomicUsize::new(0);
static KERNEL: AtomicUsize = AtomicUsize::new(0);
static TABLE: AtomicUsize = AtomicUsize::new(0);

struct Row {
    name: &'static str,
    mark: &'static str,
}

fn rows() -> Vec<Row> {
    SAFE_TEXT
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (name, mark) = line.split_once('\t').expect("name<TAB>mark");
            Row { name, mark }
        })
        .collect()
}

#[test]
fn every_safe_line_carries_a_verified_or_unverified_mark() {
    let rows = rows();
    let mut verified = 0;
    let mut unverified = 0;
    for row in &rows {
        assert!(
            row.mark.starts_with("VERIFIED:") || row.mark.starts_with("UNVERIFIED:"),
            "{}: {}",
            row.name,
            row.mark
        );
        if row.mark.starts_with("VERIFIED:") {
            verified += 1;
        } else {
            unverified += 1;
        }
    }
    assert_eq!(verified + unverified, rows.len());
    assert!(verified > 180, "verified {verified}");
    println!("async-signal-safe: {verified} VERIFIED, {unverified} UNVERIFIED");
}

#[test]
fn lookup_matches_the_file() {
    let rows = rows();
    for row in &rows {
        assert!(is_async_signal_safe(row.name), "{}", row.name);
    }
    assert!(!is_async_signal_safe("malloc"));
    assert!(!is_async_signal_safe(""));
    assert!(!is_async_signal_safe("Write"));
}

#[test]
fn not_safe_names_are_rejected() {
    for line in NOT_SAFE_TEXT.lines() {
        let name = line.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        assert!(!is_async_signal_safe(name), "{name} must not be safe");
    }
}

#[test]
fn the_signal_paths_do_not_call_the_not_safe_names() {
    for (file, source) in [
        ("entry.rs", ENTRY),
        ("deliver.rs", DELIVER),
        ("critical.rs", CRITICAL),
        ("mask.rs", MASK_SRC),
    ] {
        for line in NOT_SAFE_TEXT.lines() {
            let name = line.trim();
            if name.is_empty() || name.starts_with('#') {
                continue;
            }
            let call = format!("{name}(");
            assert!(
                !source.contains(&call),
                "{file} calls the non-async-signal-safe `{name}`"
            );
        }
    }
}

extern "C" fn safe_functions(
    sig: i32,
    info: *const SiginfoT,
    _context: *mut raven_libc_abi::structs::UcontextT,
) {
    CALLS.fetch_add(1, Ordering::SeqCst);
    let _ = sig;
    // SAFETY: the cells hold the live test kernel/table for the duration of the test.
    let kernel = unsafe { &*(KERNEL.load(Ordering::SeqCst) as *const HostSigKernel) };
    let table = unsafe { &*(TABLE.load(Ordering::SeqCst) as *const DispTable) };
    let mut set = SigSet::empty();
    set.add(2).unwrap();
    set.fill();
    set.del(2).unwrap();
    assert!(set.is_member(1).unwrap());
    let mut buffer = [0u8; raven_libc_signal::sig2str::SIG2STR_MAX];
    raven_libc_signal::sig2str::sig2str(15, &mut buffer).unwrap();
    assert_eq!(str2sig(b"TERM"), Ok(15));
    assert_eq!(names::name_of(15), Some("TERM"));
    let mut pending = SigSet::empty();
    raven_libc_signal::sigpending(kernel, &mut pending).unwrap();
    mask::set_mask(kernel, 0);
    raven_libc_signal::action::sigaction(table, 1, None, None).unwrap();
    raven_libc_signal::raise(kernel, table, 17).unwrap();
    raven_libc_signal::sigqueue(kernel, 99, 40, Sigval::ZERO).ok();
    raven_libc_signal::kill(kernel, 99, 0).ok();
    let _ = info;
}

#[test]
fn the_crate_functions_are_callable_from_a_handler() {
    let _guard = LOCK.lock().unwrap();
    CALLS.store(0, Ordering::SeqCst);
    let kernel = Box::new(HostSigKernel::new());
    let table = Box::new(DispTable::new());
    KERNEL.store(&*kernel as *const HostSigKernel as usize, Ordering::SeqCst);
    TABLE.store(&*table as *const DispTable as usize, Ordering::SeqCst);
    mask::set_mask(&*kernel, 0);
    let mut action = Disposition::handler(safe_functions as *const () as usize);
    action.flags = SA_SIGINFO;
    table.write(10, action);
    raven_libc_signal::raise(&*kernel, &*table, 10).unwrap();
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
}
