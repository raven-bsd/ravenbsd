// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S82 aarch64 dynamic-exec fixture: a static service binary that replaces itself with an
//! embedded aarch64 `ET_DYN` + `PT_INTERP` image through `PROC_EXEC`.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;

entry!(main);

/// The normally linked aarch64 PIE built by `build.sh` before this crate is compiled.
static DYNAMIC: &[u8] = include_bytes!(env!("DYNAMIC_EXEC_IMAGE"));

fn main(_args: raven_rt::Args) -> i32 {
    println!("dynamic-exec: launching");
    let argv: [&[u8]; 1] = [&b"/bin/dynamic-exec"[..]];
    match raven_rt::exec::exec_image(DYNAMIC, b"/bin/dynamic-exec", &argv, &[], &[]) {
        Ok(()) => 0,
        Err(error) => {
            println!("dynamic-exec: exec failed {:?}", error);
            1
        }
    }
}
