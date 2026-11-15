// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B71a spawn-smoke child: prints one line and exits 7.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    println!("spawn-smoke: child");
    7
}
