// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S33 one-way accounting client: 10,000 OPEN/CLOSE and DUP cycles, then a synchronous call so
//! every earlier request is complete before the process exits.

#![no_std]
#![no_main]

use raven_abi::types::Rights;
use raven_rt::entry;
use raven_rt::println;
use raven_s33_ipc::{O_RDWR, call, close, dup, open};

entry!(main);

const CYCLES: usize = 10_000;

fn main(_args: raven_rt::Args) -> i32 {
    for _ in 0..CYCLES {
        let file = match open("s33:loop", O_RDWR) {
            Ok(handle) => handle,
            Err(_) => {
                println!("s33: loop open failed");
                return 1;
            }
        };
        let duplicate = match dup(file, Rights::ALL.bits()) {
            Ok(handle) => handle,
            Err(_) => {
                println!("s33: loop dup failed");
                return 1;
            }
        };
        if close(file).is_err() || close(duplicate).is_err() {
            println!("s33: loop close failed");
            return 1;
        }
    }
    // The server is single-threaded and processes FIFO: a successful reply here proves that all
    // earlier OPEN/DUP/CLOSE requests were completed and removed from the client's list.
    let sync = match open("s33:loop", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: loop sync open failed");
            return 1;
        }
    };
    if call(sync, b"sync", None).is_err() {
        println!("s33: loop sync failed");
        return 1;
    }
    println!("s33: loop ok");
    // Leave `sync` open: process exit closes the local handle without queueing another one-way
    // request, so the exit counter measures the loop, not a trailing CLOSE.
    0
}
