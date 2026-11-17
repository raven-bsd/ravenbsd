// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `batch` command-line wrapper: queue a command for the idle queue.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(not(target_os = "none"))]
fn main() {
    std::process::exit(run());
}

#[cfg(not(target_os = "none"))]
fn run() -> i32 {
    use std::io::Read;

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = match at::parse_batch_args(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("batch: {error}");
            return 2;
        }
    };
    let text = match command {
        Some(command) => command,
        None => {
            let mut text = String::new();
            match std::io::stdin().read_to_string(&mut text) {
                Ok(_) => text,
                Err(error) => {
                    eprintln!("batch: stdin: {error}");
                    return 1;
                }
            }
        }
    };
    let mut spool = at::HostSpool::new(&at::spool_dir());
    match at::submit(&mut spool, at::now_seconds(), "now", &text, true) {
        Ok(job) => {
            println!("batch: scheduled {}", job.id);
            0
        }
        Err(error) => {
            eprintln!("batch: {error}");
            1
        }
    }
}

#[cfg(target_os = "none")]
mod program {
    use alloc::string::ToString;
    use alloc::vec::Vec;

    use at::TargetSpool;
    use raven_rt::entry;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let args = args.iter().skip(1).map(str::to_string).collect::<Vec<_>>();
        let command = match at::parse_batch_args(&args) {
            Ok(command) => command,
            Err(error) => {
                raven_rt::eprintln!("batch: {error}");
                return 2;
            }
        };
        let text = match command {
            Some(command) => command,
            None => {
                raven_rt::eprintln!(
                    "batch: standard input is unsupported on target; pass a command"
                );
                return 1;
            }
        };
        let mut spool = TargetSpool;
        match at::submit(&mut spool, at::now_seconds(), "now", &text, true) {
            Ok(job) => {
                raven_rt::println!("batch: scheduled {}", job.id);
                0
            }
            Err(error) => {
                raven_rt::eprintln!("batch: {error}");
                1
            }
        }
    }
}
