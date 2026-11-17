// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `mesg` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    const TTY: &str = "tty:console";

    fn main(args: raven_rt::Args) -> i32 {
        let action = match raven_mesg::parse_args(args.iter().skip(1)) {
            Ok(action) => action,
            Err(_) => {
                raven_rt::eprintln!("usage: mesg {}", raven_mesg::USAGE);
                return 2;
            }
        };
        let mode = match raven_rt::fs::stat(TTY) {
            Ok(stat) => stat.mode,
            Err(error) => {
                raven_rt::eprintln!("mesg: {TTY}: {}", error.description());
                return 1;
            }
        };
        match action {
            raven_mesg::Action::Query => {
                if raven_mesg::allows(mode) {
                    raven_rt::println!("y");
                    0
                } else {
                    raven_rt::println!("n");
                    1
                }
            }
            raven_mesg::Action::Set(allowed) => {
                let new_mode = raven_mesg::apply(mode, allowed);
                match raven_rt::fs::chmod(TTY, new_mode) {
                    Ok(()) => 0,
                    Err(error) => {
                        raven_rt::eprintln!("mesg: {TTY}: {}", error.description());
                        1
                    }
                }
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
