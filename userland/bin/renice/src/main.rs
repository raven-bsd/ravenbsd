// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `renice` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_rt::priority;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match raven_renice::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: renice {}", raven_renice::USAGE);
                return 2;
            }
        };
        let mut status = 0;
        for id in options.ids {
            if let Err(error) = priority::setpriority(options.which.prio(), id, options.priority) {
                raven_rt::eprintln!("renice: {id}: {}", error.description());
                status = 1;
                continue;
            }
            match priority::getpriority(options.which.prio(), id) {
                Ok(value) => raven_rt::println!("{id}: {value}"),
                Err(error) => {
                    raven_rt::eprintln!("renice: {id}: {}", error.description());
                    status = 1;
                }
            }
        }
        status
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
