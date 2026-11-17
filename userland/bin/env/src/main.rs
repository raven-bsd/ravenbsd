// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `env` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_rt::entry;

    entry!(main);

    const BASE_KEYS: [&str; 7] = ["PATH", "HOME", "USER", "LOGNAME", "SHELL", "PWD", "TERM"];

    fn base_environment(args: &raven_rt::Args) -> Vec<String> {
        let mut environment = Vec::new();
        for key in BASE_KEYS {
            if let Some(value) = args.env_get(key) {
                environment.push(format!("{key}={value}"));
            }
        }
        if args.env_get("PWD").is_none() {
            environment.push(format!("PWD={}", raven_rt::fs::cwd()));
        }
        environment
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match raven_env::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: env {}", raven_env::USAGE);
                return 2;
            }
        };
        let environment = raven_env::merge_environment(
            &base_environment(&args),
            options.ignore,
            &options.assignments,
        );
        if options.command.is_empty() {
            for entry in &environment {
                raven_rt::println!("{entry}");
            }
            return 0;
        }
        let command = &options.command[0];
        let argv: Vec<&[u8]> = options
            .command
            .iter()
            .map(|argument| argument.as_bytes())
            .collect();
        let envp: Vec<&[u8]> = environment.iter().map(|entry| entry.as_bytes()).collect();
        let mut last_error = Errno::ENOENT;
        for candidate in raven_env::candidates(command, &environment) {
            match raven_rt::exec::execve(candidate.as_bytes(), &argv, &envp, &[]) {
                Ok(()) => return 0,
                Err(Errno::ENOENT) => continue,
                Err(Errno::EACCES) => {
                    last_error = Errno::EACCES;
                }
                Err(error) => {
                    raven_rt::eprintln!("env: '{command}': {}", error.description());
                    return 126;
                }
            }
        }
        raven_rt::eprintln!("env: '{command}': {}", last_error.description());
        127
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
