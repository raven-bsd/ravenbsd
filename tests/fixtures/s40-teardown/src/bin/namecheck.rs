// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S40 name check: registering the faulting client's scheme name must succeed after its death.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::scheme_register;
use raven_s40_teardown::FAULT_SCHEME;

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    match scheme_register(FAULT_SCHEME) {
        Ok(()) => 0,
        Err(error) => {
            println!("s40: namecheck errno={}", error.0);
            1
        }
    }
}
