// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `replay list` and `replay run <target> <file>...`.

use raven_libc_fuzz_replay::{TARGETS, replay_file};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
    };
    match command.as_str() {
        "list" => {
            for (name, _) in TARGETS {
                println!("{name}");
            }
        }
        "run" => {
            let Some(target) = args.next() else {
                usage();
            };
            if !TARGETS.iter().any(|(name, _)| *name == target) {
                eprintln!("replay: unknown target {target}");
                std::process::exit(2);
            }
            let mut failure = false;
            for path in args {
                let path = std::path::Path::new(&path);
                match replay_file(&target, path) {
                    Ok(()) => println!("ok {}", path.display()),
                    Err(error) => {
                        println!("FAIL {}: {error}", path.display());
                        failure = true;
                    }
                }
            }
            std::process::exit(if failure { 1 } else { 0 });
        }
        _ => usage(),
    }
}

fn usage() -> ! {
    eprintln!("usage: replay list | replay run <target> <file>...");
    std::process::exit(2);
}
