// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! S188 shell-shape discriminator: a Tokio runtime (current-thread, the shell cfg on RavenBSD),
//! a `thread_local!` created on the main thread, and `exit(0)` from main while the runtime is
//! alive. The shell's AccessError at local.rs:428 appears here if the libc exit/teardown order
//! is the cause; a clean run points at the shell's own call path.

use std::cell::RefCell;

struct MainValue;

impl Drop for MainValue {
    fn drop(&mut self) {
        println!("TOKIO188: main tls drop");
    }
}

thread_local! {
    static MAIN_VALUE: RefCell<Option<MainValue>> = RefCell::new(Some(MainValue));
}

fn main() {
    println!("TOKIO188: start");
    let named = std::thread::Builder::new()
        .name("s188-named".into())
        .spawn(|| println!("TOKIO188: named thread body"))
        .expect("named thread");
    let _ = named.join();
    println!("TOKIO188: named thread ok");
    // The shell's cfg is the current-thread runtime on RavenBSD (entry.rs); the multi-thread
    // builder above faults in the tokio blocking-pool thread spawn at rip 0x40f121 cr2=0.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread runtime");
    MAIN_VALUE.with(|_| println!("TOKIO188: main tls set"));
    runtime.block_on(async {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    });
    println!("TOKIO188: runtime idle, exit");
    std::process::exit(0);
}
