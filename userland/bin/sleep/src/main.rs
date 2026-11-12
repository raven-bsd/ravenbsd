// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `sleep` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_sleep::{parse_seconds, to_nanos};

    entry!(main);

    const TOOL: &str = "sleep";

    fn main(args: raven_rt::Args) -> i32 {
        let mut operands = args.iter().skip(1);
        let Some(argument) = operands.next() else {
            raven_rt::eprintln!("usage: sleep <seconds> | sleep --selftest");
            return 2;
        };
        if operands.next().is_some() {
            raven_rt::eprintln!("usage: sleep <seconds> | sleep --selftest");
            return 2;
        }
        if argument == "--selftest" {
            return match raven_rt::nanosleep(1_000_000) {
                Ok(()) => {
                    raven_rt::println!("{TOOL}: selftest ok");
                    0
                }
                Err(error) => {
                    raven_rt::eprintln!("{TOOL}: {}", error.description());
                    1
                }
            };
        }
        let seconds = match parse_seconds(argument) {
            Ok(seconds) => seconds,
            Err(error) => {
                raven_rt::eprintln!("{error}");
                return 2;
            }
        };
        let Some(nanos) = to_nanos(seconds) else {
            raven_rt::eprintln!("{TOOL}: {argument}: value out of range");
            return 2;
        };
        match raven_rt::nanosleep(nanos) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: {}", error.description());
                1
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
