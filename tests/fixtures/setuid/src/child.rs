// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! U3b setuid/setgid child helper. Prints the credentials the kernel installed for the image.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use raven_setuid::self_info;

entry!(main);

fn main(args: raven_rt::Args) -> i32 {
    let label = args.get(1).unwrap_or("?");
    let mut info = [0u64; 8];
    if self_info(&mut info).is_err() {
        println!("setuid-child: fail self-info");
        return 1;
    }
    println!(
        "setuid-child: mode={} uid={} euid={} gid={} egid={}",
        label, info[4], info[5], info[6], info[7]
    );
    0
}
