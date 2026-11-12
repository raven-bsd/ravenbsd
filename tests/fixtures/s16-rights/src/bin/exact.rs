// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S16 exact-name client: this process is created after the server registers `exact`, so its
//! snapshot contains the exact name and it needs no prefix.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use raven_s16_rights::{O_READ, close, open};

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    match open("exact:x", O_READ) {
        Ok(handle) => {
            let _ = close(handle);
            println!("s16: exact name ok");
            0
        }
        Err(error) => {
            println!("s16: exact errno={}", error.0);
            1
        }
    }
}
