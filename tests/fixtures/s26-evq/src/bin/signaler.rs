// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S26 third process: signals the transferred user event and waits for the server's ack event.

#![no_std]
#![no_main]

use raven_abi::types::Handle;
use raven_rt::entry;
use raven_s26_evq::{EvqEvent, SIGNALED, event_signal, evq_create, evq_ctl, evq_wait};

entry!(main);

/// Handles transferred by the controller before this process starts.
const USER_EVENT: Handle = Handle::new(3, 0);
const ACK_EVENT: Handle = Handle::new(4, 0);

const ADD: usize = 1;
const ACK_COOKIE: u64 = 0xA1;
const EVENTS: u64 = 77_997;

fn main(_args: raven_rt::Args) -> i32 {
    let queue = match evq_create() {
        Ok(queue) => queue,
        Err(_) => return 40,
    };
    if evq_ctl(queue, ADD, ACK_EVENT, SIGNALED, ACK_COOKIE).is_err() {
        return 41;
    }
    let mut output = [EvqEvent::default(); 1];
    let mut index = 0u64;
    while index < EVENTS {
        if event_signal(USER_EVENT).is_err() {
            return 42;
        }
        let count = match evq_wait(queue, &mut output, 5_000_000_000) {
            Ok(count) => count,
            Err(_) => return 43,
        };
        if count != 1 || output[0].cookie != ACK_COOKIE || output[0].events != SIGNALED as u32 {
            return 44;
        }
        index = index.saturating_add(1);
    }
    0
}
