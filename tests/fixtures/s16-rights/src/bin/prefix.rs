// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S16 prefix client. Its snapshot predates the server registration; the controller grants the
//! `s16a` prefix before start, removes it mid-run, and the existing file handle must keep working.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::println;
use raven_s16_rights::{O_READ, close, nanosleep, open, read_byte};

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    let one = match open("s16a:one", O_READ) {
        Ok(handle) => handle,
        Err(error) => {
            println!("s16: prefix first errno={}", error.0);
            return 1;
        }
    };
    println!("s16: prefix open ok");
    match open("other:x", O_READ) {
        Err(Errno::EACCES) => println!("s16: prefix denied"),
        _ => {
            println!("s16: prefix other leaked");
            return 1;
        }
    }
    // The controller removes the prefix while this sleeps.
    nanosleep(800_000_000);
    match open("s16a:two", O_READ) {
        Err(Errno::EACCES) => println!("s16: prefix removed"),
        _ => {
            println!("s16: prefix still visible");
            return 1;
        }
    }
    match read_byte(one) {
        Ok(b'R') => println!("s16: existing handle ok"),
        _ => {
            println!("s16: existing read failed");
            return 1;
        }
    }
    let _ = close(one);
    0
}
