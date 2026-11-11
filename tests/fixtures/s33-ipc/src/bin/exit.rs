// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S33 cancellation client: queue a one-way CLOSE while the server is paused, then exit.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use raven_s33_ipc::{O_RDWR, call, close, open};

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    let file = match open("s33:pause", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: exit open failed");
            return 1;
        }
    };
    if call(file, b"pause", None).is_err() {
        println!("s33: exit pause failed");
        return 1;
    }
    // The server replied and is now asleep. The CLOSE is queued behind it; this process exits
    // before the server wakes, so the kernel cancels the request on process exit.
    if close(file).is_err() {
        println!("s33: exit close failed");
        return 1;
    }
    println!("s33: exit client done");
    0
}
