// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `getconf` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match raven_getconf::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: getconf {}", raven_getconf::USAGE);
                return 2;
            }
        };
        if options.all {
            for entry in raven_getconf::ENTRIES {
                raven_rt::println!("{}", raven_getconf::all_line(entry));
            }
            return 0;
        }
        let mut status = 0;
        for name in options.names {
            match raven_getconf::lookup(&name) {
                Some(value) => raven_rt::println!("{value}"),
                None => {
                    raven_rt::eprintln!("getconf: {name}: unrecognized variable");
                    status = 1;
                }
            }
        }
        status
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
