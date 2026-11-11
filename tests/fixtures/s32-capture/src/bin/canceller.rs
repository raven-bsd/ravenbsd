// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S32 cancellation client: queue a one-way CLOSE while the server is paused, then exit.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use raven_s32_capture::{O_RDWR, call, close, open};

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    let file = match open("s32:pause", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s32: canceller open failed");
            return 1;
        }
    };
    if call(file, b"pause", None).is_err() {
        println!("s32: canceller pause failed");
        return 1;
    }
    // The server replied and is asleep. The CLOSE is queued; this process exits and the kernel
    // cancels the request on process exit.
    if close(file).is_err() {
        println!("s32: canceller close failed");
        return 1;
    }
    println!("s32: canceller done");
    0
}
