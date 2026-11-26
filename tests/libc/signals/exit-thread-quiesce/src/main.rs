// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! S194 red test: one thread calls `exit(0)` while a sibling keeps executing user code that
//! touches a `thread_local!`. The kernel must stop the sibling before it destroys the address
//! space; otherwise the sibling reads TLS memory that is already being torn down and std panics
//! with "cannot access a Thread Local Storage value during or after destruction".

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

thread_local! {
    static VALUE: Cell<u64> = const { Cell::new(7) };
}

static READY: AtomicBool = AtomicBool::new(false);

fn main() {
    let _worker = thread::Builder::new()
        .name("s194-worker".into())
        .spawn(|| {
            VALUE.with(|value| {
                value.set(11);
                println!("QUIESCE: worker ready");
            });
            READY.store(true, Ordering::Release);
            let mut spins = 0u64;
            loop {
                VALUE.with(|value| {
                    spins = spins.wrapping_add(value.get());
                });
                std::hint::black_box(spins);
            }
        })
        .expect("worker");
    while !READY.load(Ordering::Acquire) {
        thread::yield_now();
    }
    // The worker is round-robin placed on another CPU; give it a slice to prove it runs
    // concurrently with the thread that is about to call exit.
    thread::sleep(Duration::from_millis(100));
    println!("QUIESCE: main exit");
    std::process::exit(0);
}
