// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal target shell for the fixture.
//!
//! `su` without `-c` starts this binary as the fallback `/bin/brush`; `su -c <path>` starts it
//! with `-c <path>` and it runs that single probe path. Every successful start writes
//! `/tmp/shell-ran`, so the parent can prove a failed authentication did not start a shell.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_user_secure_fixture::{
        exit_code, print_environment, print_groups, print_ids, spawn_plain, write_marker,
    };

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let arguments: Vec<&str> = args.iter().skip(1).collect();
        write_marker();
        if arguments.first() == Some(&"-c") {
            let Some(command) = arguments.get(1).copied() else {
                raven_rt::println!("secure-shell: fail missing-command");
                return 2;
            };
            let environment = raven_user_secure_fixture::inherited_environment(&args);
            let environment: Vec<&str> = environment.iter().map(String::as_str).collect();
            let image = match raven_rt::map_initfs() {
                Ok(image) => image,
                Err(_) => {
                    raven_rt::println!("secure-shell: fail initfs-map");
                    return 3;
                }
            };
            let initfs = match Initfs::parse(image) {
                Ok(initfs) => initfs,
                Err(_) => {
                    raven_rt::println!("secure-shell: fail initfs-parse");
                    return 3;
                }
            };
            let child = match spawn_plain(&initfs, command, &[command], &environment) {
                Ok(child) => child,
                Err(_) => {
                    raven_rt::println!("secure-shell: fail spawn {command}");
                    return 4;
                }
            };
            let code = exit_code(child);
            if code != 0 {
                raven_rt::println!("secure-shell: command-exit={code}");
            }
            return code;
        }

        raven_rt::println!("secure-shell: ran");
        if print_ids("secure-shell").is_err() || print_groups("secure-shell").is_err() {
            raven_rt::println!("secure-shell: fail probe");
            return 5;
        }
        print_environment("secure-shell", &args);
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
