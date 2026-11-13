// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The R-17 allocation probes: qsort, bsearch, getenv and the first 32 `atexit` registrations.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use raven_libc_stdlib::env::EnvTable;
use raven_libc_stdlib::exit::{Entry, ExitTable, Kind};
use raven_libc_stdlib::{bsearch, qsort};

struct Counting;

thread_local! {
    /// Per-thread allocation counter, so parallel test threads cannot pollute a probe.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

fn reset_allocations() {
    ALLOCATIONS.with(|cell| cell.set(0));
}

// SAFETY: every method forwards to the system allocator with the same layout and only bumps a
// thread-local counter; the counter does not observe or alter the allocation.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|cell| cell.set(cell.get().wrapping_add(1)));
        // SAFETY: the caller guarantees a valid layout for the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout came from `System.alloc` above.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|cell| cell.set(cell.get().wrapping_add(1)));
        // SAFETY: the pointer, layout and size follow the `GlobalAlloc` contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn cmp(a: &[u8], b: &[u8]) -> i32 {
    let left = i32::from_le_bytes([a[0], a[1], a[2], a[3]]);
    let right = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    match left.cmp(&right) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

#[test]
fn qsort_does_not_allocate() {
    let mut buffer = Vec::new();
    for value in (0..1000i32).rev() {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    reset_allocations();
    qsort(&mut buffer, 4, cmp);
    assert_eq!(allocations(), 0);
}

#[test]
fn bsearch_does_not_allocate() {
    let mut buffer = Vec::new();
    for value in 0..1000i32 {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    let key = 500i32.to_le_bytes();
    reset_allocations();
    let found = bsearch(&buffer, 4, |element| {
        let value = i32::from_le_bytes([element[0], element[1], element[2], element[3]]);
        match i32::from_le_bytes([key[0], key[1], key[2], key[3]]).cmp(&value) {
            core::cmp::Ordering::Less => -1,
            core::cmp::Ordering::Equal => 0,
            core::cmp::Ordering::Greater => 1,
        }
    });
    assert!(found.is_some());
    assert_eq!(allocations(), 0);
}

#[test]
fn strtol_does_not_allocate() {
    reset_allocations();
    assert_eq!(
        raven_libc_stdlib::strtol(b"-0x7fffffff", 0).0,
        -2_147_483_647
    );
    assert_eq!(allocations(), 0);
}

#[test]
fn first_32_atexit_registrations_do_not_allocate() {
    let mut table = ExitTable::new();
    reset_allocations();
    for func in 1..=32usize {
        table
            .register(Entry {
                func,
                arg: 0,
                dso: 0,
                kind: Kind::Atexit,
            })
            .unwrap();
    }
    assert_eq!(allocations(), 0);
    assert_eq!(table.pending(), 32);
    table
        .register(Entry {
            func: 33,
            arg: 0,
            dso: 0,
            kind: Kind::Atexit,
        })
        .unwrap();
    assert!(allocations() > 0);
}

#[test]
fn getenv_does_not_allocate() {
    let initial: &'static [u8] = Box::leak(b"A=value\0".to_vec().into_boxed_slice());
    let mut table = EnvTable::new();
    table.adopt_initial(&[initial]);
    reset_allocations();
    let value = table.getenv(b"A");
    assert_eq!(value, Some(b"value".as_slice()));
    assert_eq!(allocations(), 0);
}
