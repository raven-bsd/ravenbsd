// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S16 CLEAR client. Its snapshot predates the server; the controller grants two prefixes, then
//! clears all prefixes mid-run. Both schemes become unreachable for new opens, while the file
//! handles opened before the clear stay usable.

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
            println!("s16: clear first errno={}", error.0);
            return 1;
        }
    };
    println!("s16: clear first ok");
    let other = match open("other:x", O_READ) {
        Ok(handle) => handle,
        Err(error) => {
            println!("s16: clear other errno={}", error.0);
            return 1;
        }
    };
    println!("s16: clear other ok");
    // The controller clears both prefixes while this sleeps.
    nanosleep(800_000_000);
    if !matches!(open("s16a:two", O_READ), Err(Errno::EACCES)) {
        println!("s16: clear s16a still visible");
        return 1;
    }
    if !matches!(open("other:y", O_READ), Err(Errno::EACCES)) {
        println!("s16: clear other still visible");
        return 1;
    }
    println!("s16: clear all ok");
    match (read_byte(one), read_byte(other)) {
        (Ok(b'R'), Ok(b'R')) => println!("s16: clear existing ok"),
        _ => {
            println!("s16: clear existing failed");
            return 1;
        }
    }
    let _ = close(one);
    let _ = close(other);
    0
}
