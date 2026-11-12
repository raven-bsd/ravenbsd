// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S39 fixture client: opens the granted scheme through its prefix, refuses the denied scheme,
//! registers an event and waits until the controller kills it.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::println;
use raven_s39_parity::{
    EVENT_COOKIE, EVQ_ADD, EvqEvent, SIGNALED, event_create, evq_create, evq_ctl, evq_wait,
    open_retry,
};
use raven_scheme::Client;

entry!(main);

const O_RDONLY: u32 = 1;
const RETRIES: usize = 200_000;
const GRANTED_PATH: &str = "b47a:ctl";
const DENIED_PATH: &str = "b47b:ctl";

fn fail(message: &str) -> i32 {
    println!("s39: client failed: {message}");
    1
}

fn main(_args: raven_rt::Args) -> i32 {
    let api = Client::new();

    // The denied scheme is registered by the server but is outside the client's granted prefix;
    // wait until registration makes the lookup distinguishable from ENOENT.
    let mut attempt = 0usize;
    loop {
        match api.open(DENIED_PATH, O_RDONLY) {
            Err(Errno::EACCES) => {
                println!("s39: namespace denied ok");
                break;
            }
            Err(Errno::ENOENT) | Err(Errno::EIO) => {
                core::hint::spin_loop();
                attempt = attempt.saturating_add(1);
                if attempt >= RETRIES {
                    return fail("denied scheme did not register");
                }
            }
            Ok(_) => return fail("namespace leak"),
            Err(_) => return fail("denied open"),
        }
    }

    let control = match open_retry(GRANTED_PATH, O_RDONLY) {
        Ok(handle) => handle,
        Err(_) => return fail("granted open"),
    };
    println!("s39: prefix open ok");

    let queue = match evq_create() {
        Ok(queue) => queue,
        Err(_) => return fail("evq create"),
    };
    let event = match event_create() {
        Ok(event) => event,
        Err(_) => return fail("event create"),
    };
    if evq_ctl(queue, EVQ_ADD, event, SIGNALED, EVENT_COOKIE).is_err() {
        return fail("evq register");
    }
    println!("s39: client event registered");

    let mut reply = [0u8; 8];
    match api.call(control, b"armed", &mut reply) {
        Ok(length) if length == 2 && &reply[..2] == b"ok" => {}
        _ => return fail("armed call"),
    }
    println!("s39: client ready");

    // No ready source: this blocks until the controller terminates the process.  The exit hook
    // must clear the registration and wake this waiter.
    let mut records = [EvqEvent::default(); 1];
    let _ = evq_wait(queue, &mut records, 0);
    fail("event wait returned")
}
