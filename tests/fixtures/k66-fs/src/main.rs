// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K66 contract fixture entry point.

#![no_std]
#![no_main]
#![forbid(unsafe_code)]

use raven_rt::entry;
use raven_rt::println;

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    match raven_k66_fs::run_contract() {
        Ok(()) => {
            println!("k66-fs: contract ok");
            0
        }
        Err(message) => {
            println!("k66-fs: failed: {message}");
            1
        }
    }
}
