// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S26 write client: streams writes to the scheme, then observes the server-death HUP edge.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_s26_evq::{EDGE, EvqEvent, HUP, O_RDWR, evq_create, evq_ctl, evq_wait, open, write};

entry!(main);

const ADD: usize = 1;
const FILE_COOKIE: u64 = 0xC1;
const WRITES: u64 = 5_000;

fn main(_args: raven_rt::Args) -> i32 {
    let file = match open("evq:test", O_RDWR) {
        Ok(file) => file,
        Err(_) => return 30,
    };
    let queue = match evq_create() {
        Ok(queue) => queue,
        Err(_) => return 31,
    };
    if evq_ctl(queue, ADD, file, HUP | EDGE, FILE_COOKIE).is_err() {
        return 32;
    }
    let mut index = 0u64;
    while index < WRITES {
        match write(file, b"x") {
            Ok(1) => index = index.saturating_add(1),
            _ => return 33,
        }
    }

    let mut output = [EvqEvent::default(); 1];
    let count = match evq_wait(queue, &mut output, 300_000_000_000) {
        Ok(count) => count,
        Err(_) => return 34,
    };
    if count != 1 || output[0].cookie != FILE_COOKIE || output[0].events != HUP as u32 {
        return 35;
    }
    // The HUP is an edge: the still-set level must not be reported a second time.
    let count = match evq_wait(queue, &mut output, 10_000_000) {
        Ok(count) => count,
        Err(_) => return 36,
    };
    if count != 0 {
        return 37;
    }
    0
}
