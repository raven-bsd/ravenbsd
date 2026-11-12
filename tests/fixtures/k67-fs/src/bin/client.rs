// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K67 abandoned-open client: opens until the server budget rejects, then exits
//! without closing anything.  Process death must close every reference.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Client;

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    let client = Client::new();
    let mut accepted = 0usize;
    for _ in 0..1_000 {
        match client.open("ramfs:cycle", 1) {
            Ok(_) => accepted += 1,
            Err(Errno::EMFILE) => break,
            Err(_) => return 1,
        }
    }
    println!("k67: client accepted {accepted}");
    if accepted != 256 {
        return 2;
    }
    0
}
