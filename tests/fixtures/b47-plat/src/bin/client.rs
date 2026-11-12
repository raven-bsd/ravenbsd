// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot GPIO client: claim pin 27 and exit without closing it.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Client;

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    match Client::new().open("plat:gpio/27", 1) {
        Ok(_handle) => {
            println!("b47-plat: client claimed");
            0
        }
        Err(error) => {
            println!("b47-plat: client claim failed: {}", error.description());
            1
        }
    }
}
