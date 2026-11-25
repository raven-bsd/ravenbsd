// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! S194 diagnostic: a sibling thread initialises a `thread_local!`, exits, and the main thread
//! reads the same key afterwards. Each thread must own its TLS block: the sibling's destructor
//! must not leave the main thread's key in the destroyed state. The shell's AccessError is the
//! same failure when a runtime worker retires under a live main thread.

use std::cell::Cell;
use std::thread;

thread_local! {
    static VALUE: Cell<u64> = const { Cell::new(7) };
}

fn main() {
    VALUE.with(|value| {
        value.set(41);
        println!("SIBLING: main before value={}", value.get());
    });

    let worker = thread::Builder::new()
        .name("s194-sibling".into())
        .spawn(|| {
            VALUE.with(|value| {
                value.set(11);
                println!("SIBLING: worker value={}", value.get());
            });
        })
        .expect("worker");
    worker.join().expect("join");

    // The sibling is gone and its TLS destructors have run. These keys belong to this thread.
    VALUE.with(|value| println!("SIBLING: main after value={}", value.get()));
    println!("SIBLING: pass");
    std::process::exit(0);
}
