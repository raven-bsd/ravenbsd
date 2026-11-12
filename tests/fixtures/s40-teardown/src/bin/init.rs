// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S40 controller: repeat the fault-with-pending-request scenario and check every invariant.

#![no_std]
#![no_main]

use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::{call, close, map_initfs};
use raven_s40_teardown::{HOLD_PATH, SYNC_PATH, open_retry, proc_wait, spawn_path};

entry!(main);

/// Repetitions required by the ticket.
const REPS: usize = 100;
/// The page-fault exit code used by `process::page_fault`.
const FAULT_CODE: i32 = -11;

fn fail(message: &str) -> i32 {
    println!("s40: failed {message}");
    1
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs() {
        Ok(image) => image,
        Err(_) => return fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => return fail("initfs parse"),
    };
    let _server = match spawn_path(&initfs, "/server") {
        Ok(server) => server,
        Err(_) => return fail("server spawn"),
    };
    // The probe completes only after the server registered `s40`.
    let probe = match open_retry(HOLD_PATH) {
        Ok(probe) => probe,
        Err(_) => return fail("server register"),
    };
    let _ = close(probe);

    let mut cycle = 0usize;
    while cycle < REPS {
        let client = match spawn_path(&initfs, "/s40-fault") {
            Ok(client) => client,
            Err(_) => return fail("fault spawn"),
        };
        match proc_wait(client) {
            Ok(FAULT_CODE) => {}
            Ok(code) => {
                println!("s40: fault exit {code}");
                return 1;
            }
            Err(_) => return fail("fault wait"),
        }
        let _ = close(client);
        // The faulting client registered `s40-fault`; its teardown must release the name.
        let check = match spawn_path(&initfs, "/s40-namecheck") {
            Ok(check) => check,
            Err(_) => return fail("namecheck spawn"),
        };
        match proc_wait(check) {
            Ok(0) => {}
            Ok(code) => {
                println!("s40: namecheck exit {code}");
                return 1;
            }
            Err(_) => return fail("namecheck wait"),
        }
        let _ = close(check);
        cycle = cycle.saturating_add(1);
    }

    // FIFO barrier: the server processes this after every held WRITE, so all requests (and their
    // captures) are retired before the final evidence prints.
    let sync = match open_retry(SYNC_PATH) {
        Ok(sync) => sync,
        Err(_) => return fail("sync open"),
    };
    let mut response = [0u8; 1];
    match call(sync, b"", &mut response) {
        Ok(_) => {}
        Err(error) => {
            println!("s40: failed sync call errno={}", error.0);
            return 1;
        }
    }
    let _ = close(sync);
    // The CLOSE is one-way: a final OPEN queued behind it proves the server processed the
    // CLOSE, so the controller has no outstanding client request when it exits.
    let _barrier = match open_retry(SYNC_PATH) {
        Ok(barrier) => barrier,
        Err(_) => return fail("final sync open"),
    };

    println!("s40: cycles done={REPS}");
    println!("s40: teardown ok");
    0
}
