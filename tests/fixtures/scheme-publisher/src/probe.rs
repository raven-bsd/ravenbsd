// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Short-lived uid 1000 probe: `file`/`vfs` must be denied before boot registration.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use raven_scheme_publisher::scheme_register;

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    let file = scheme_register("file")
        .map(|()| "registered")
        .unwrap_or("EPERM");
    let vfs = scheme_register("vfs")
        .map(|()| "registered")
        .unwrap_or("EPERM");
    println!("unprivileged-boot: file={file} vfs={vfs}");
    0
}
