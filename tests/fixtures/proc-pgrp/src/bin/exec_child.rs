// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K488 exec child: signal the parent that exec completed, then stay alive.

#![no_std]
#![no_main]

use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::sys;

entry!(main);

fn main(args: raven_rt::Args) -> i32 {
    let handle = args
        .get(1)
        .and_then(|text| text.parse::<u32>().ok())
        .unwrap_or(0);
    let _ = sys::write(Handle(handle), &[1]);
    loop {
        let _ = sys::nanosleep(1_000_000_000);
    }
}
