// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! The shell registry: LIFO order, the spill, DSO selection and the C return convention.

use super::*;

use std::sync::Mutex;

use raven_libc_abi::errno::ENOMEM;

static LIFO_LOG: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static SPILL_LOG: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static DSO_LOG: Mutex<Vec<usize>> = Mutex::new(Vec::new());

extern "C" fn noop() {}

extern "C" fn lifo_one() {
    LIFO_LOG.lock().unwrap().push(1);
}

extern "C" fn lifo_two() {
    LIFO_LOG.lock().unwrap().push(2);
}

extern "C" fn lifo_three() {
    LIFO_LOG.lock().unwrap().push(3);
}

extern "C" fn spill_record(arg: *mut c_void) {
    SPILL_LOG.lock().unwrap().push(arg as usize);
}

extern "C" fn dso_record(arg: *mut c_void) {
    DSO_LOG.lock().unwrap().push(arg as usize);
}

fn cxa(func: extern "C" fn(*mut c_void), arg: usize, dso: usize) -> Entry {
    Entry {
        func: func as usize,
        arg,
        dso,
        kind: Kind::Cxa,
    }
}

#[test]
fn newest_registration_runs_first_and_only_once() {
    LIFO_LOG.lock().unwrap().clear();
    let registry = Registry::new();
    assert_eq!(status(registry.register(atexit_entry(lifo_one))), 0);
    assert_eq!(status(registry.register(atexit_entry(lifo_two))), 0);
    assert_eq!(status(registry.register(atexit_entry(lifo_three))), 0);
    assert_eq!(registry.pending(), 3);
    registry.run_all();
    assert_eq!(*LIFO_LOG.lock().unwrap(), vec![3, 2, 1]);
    assert_eq!(registry.pending(), 0);
    registry.run_all();
    assert_eq!(*LIFO_LOG.lock().unwrap(), vec![3, 2, 1]);
}

#[test]
fn the_spill_keeps_lifo_and_passes_the_argument() {
    SPILL_LOG.lock().unwrap().clear();
    let registry = Registry::new();
    for value in 1..=40usize {
        let result = registry.register(cxa(spill_record, value, 0));
        assert_eq!(status(result), 0);
    }
    assert_eq!(registry.pending(), 40);
    registry.run_all();
    assert_eq!(
        *SPILL_LOG.lock().unwrap(),
        (1..=40usize).rev().collect::<Vec<_>>()
    );
}

#[test]
fn finalize_dso_runs_only_that_handle_and_null_runs_the_rest() {
    DSO_LOG.lock().unwrap().clear();
    let registry = Registry::new();
    for (value, dso) in [(1usize, 0x100usize), (2, 0x200), (3, 0x100), (4, 0)] {
        assert_eq!(status(registry.register(cxa(dso_record, value, dso))), 0);
    }
    registry.finalize(0x100usize as *mut c_void);
    assert_eq!(*DSO_LOG.lock().unwrap(), vec![3, 1]);
    assert_eq!(registry.pending(), 2);
    registry.finalize(0x300usize as *mut c_void);
    assert_eq!(*DSO_LOG.lock().unwrap(), vec![3, 1]);
    registry.finalize(core::ptr::null_mut());
    assert_eq!(*DSO_LOG.lock().unwrap(), vec![3, 1, 4, 2]);
    assert_eq!(registry.pending(), 0);
}

#[test]
fn atexit_and_cxa_atexit_report_success() {
    let registry = Registry::new();
    assert_eq!(status(registry.register(atexit_entry(noop))), 0);
    assert_eq!(
        status(registry.register(cxa_entry(
            Some(spill_record),
            core::ptr::null_mut(),
            core::ptr::null_mut()
        ))),
        0
    );
    assert_eq!(registry.pending(), 2);
    assert_eq!(
        status(registry.register(cxa_entry(
            None,
            core::ptr::null_mut(),
            core::ptr::null_mut()
        ))),
        0
    );
    assert_eq!(registry.pending(), 2);
}

#[test]
fn enomem_maps_to_minus_one_and_sets_errno() {
    let _guard = crate::ffi::TestLock::acquire();
    crate::ffi::errno::set_raw(42);
    assert_eq!(status(Ok(())), 0);
    assert_eq!(crate::ffi::errno::get(), 42);
    assert_eq!(status(Err(Errno(ENOMEM as u16))), -1);
    assert_eq!(crate::ffi::errno::get(), ENOMEM);
}
