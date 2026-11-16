// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `nohup` utility.
//!
//! The kernel queues signals and the exec'd child does not install a SIGHUP handler, so a
//! queued hangup does not terminate it. The runtime has no stdout descriptor or `isatty` path;
//! this utility therefore implements the non-terminal case: it execs the command without
//! redirecting output and preserves the command's exit status. The POSIX terminal-only
//! `nohup.out` redirect is unavailable until a stdout/fd API exists.

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

    fn run(command: &[String], environment: &[String]) -> i32 {
        let program = &command[0];
        let argv: Vec<&[u8]> = command.iter().map(|argument| argument.as_bytes()).collect();
        let envp: Vec<&[u8]> = environment.iter().map(|entry| entry.as_bytes()).collect();
        let mut last_error = Errno::ENOENT;
        for candidate in raven_env::candidates(program, environment) {
            match raven_rt::exec::execve(candidate.as_bytes(), &argv, &envp, &[]) {
                Ok(()) => return 0,
                Err(Errno::ENOENT) => continue,
                Err(Errno::EACCES) => last_error = Errno::EACCES,
                Err(error) => {
                    raven_rt::eprintln!("nohup: '{program}': {}", error.description());
                    return 126;
                }
            }
        }
        raven_rt::eprintln!("nohup: '{program}': {}", last_error.description());
        127
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match raven_nohup::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: nohup {}", raven_nohup::USAGE);
                return 2;
            }
        };
        let environment = raven_env::merge_environment(&base_environment(&args), false, &[]);
        run(&options.command, &environment)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
