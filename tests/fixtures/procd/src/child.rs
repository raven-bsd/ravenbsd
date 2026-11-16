// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2e procd child: prints the argv/environment handed to it, then exits or sleeps.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let mode = args.get(1).unwrap_or("missing");
        match mode {
            "exec" => {
                let test = args.env_get("TEST").unwrap_or("<unset>");
                raven_rt::println!("procd-child: exec TEST={test}");
                7
            }
            "sleep" => {
                raven_rt::println!("procd-child: sleeping");
                loop {
                    let _ = raven_rt::nanosleep(50_000_000);
                }
            }
            _ => {
                raven_rt::println!("procd-child: unknown mode={mode}");
                2
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
