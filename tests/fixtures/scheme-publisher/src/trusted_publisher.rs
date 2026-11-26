// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Trusted root publisher used by the protected-name restart row.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use raven_scheme_publisher::scheme_register;

entry!(main);

fn main(args: raven_rt::Args) -> i32 {
    let label = args.get(1).unwrap_or("?");
    let mut ok = true;
    for name in ["file", "vfs"] {
        match scheme_register(name) {
            Ok(()) => {}
            Err(error) => {
                println!("trusted: {label} {name}=errno{}", error.0);
                ok = false;
            }
        }
    }
    if ok {
        println!("trusted: {label} file=ok vfs=ok");
        0
    } else {
        1
    }
}
