// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `tput` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::vec::Vec;

    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use tput::{Value, capability};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let mut term = args.env_get("TERM").unwrap_or("xterm");
        let mut name: Option<&str> = None;
        let mut parameters = Vec::new();
        let mut iter = args.iter().skip(1);
        while let Some(arg) = iter.next() {
            match arg {
                "-T" => term = iter.next().unwrap_or(""),
                "-S" | "--help" => {
                    raven_rt::eprintln!("{}", usage("tput", "[-T type] capname [parameters]"));
                    return EXIT_USAGE;
                }
                other if other.starts_with('-') => {
                    raven_rt::eprintln!("{}", usage("tput", "[-T type] capname [parameters]"));
                    return EXIT_USAGE;
                }
                other if name.is_none() => name = Some(other),
                other => match other.parse::<u16>() {
                    Ok(value) => parameters.push(value),
                    Err(_) => {
                        raven_rt::eprintln!("{}", usage("tput", "[-T type] capname [parameters]"));
                        return EXIT_USAGE;
                    }
                },
            }
        }
        let Some(name) = name else {
            raven_rt::eprintln!("{}", usage("tput", "[-T type] capname [parameters]"));
            return EXIT_USAGE;
        };
        match capability(term, name, &parameters) {
            Ok(Value::Bytes(bytes)) => {
                raven_rt::print!("{bytes}");
                EXIT_OK
            }
            Ok(Value::Number(number)) => {
                raven_rt::print!("{number}");
                EXIT_OK
            }
            Err(_) => {
                raven_rt::eprintln!("tput: unknown capability: {name}");
                EXIT_ERROR
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
