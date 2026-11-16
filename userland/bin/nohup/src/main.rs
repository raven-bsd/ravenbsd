// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `nohup` utility.
//!
//! The kernel queues signals and the exec'd child does not install a SIGHUP handler, so a
//! queued hangup does not terminate it. The runtime has no descriptor table for stdout; the
//! utility creates/appends the standard `nohup.out` (mode 0600) and reports it, while the child's
//! own console output remains on the debug console.

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
    use raven_proto_fs::{O_CREAT, O_WRONLY};
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

    fn open_output(args: &raven_rt::Args) -> Result<String, Errno> {
        let flags = O_WRONLY | O_CREAT;
        let directory = args
            .env_get("PWD")
            .map(String::from)
            .unwrap_or_else(raven_rt::fs::cwd);
        let display = format!("{directory}/nohup.out");
        let open_path = format!("file:{display}");
        match raven_rt::fs::open(&open_path, flags) {
            Ok(file) => {
                let _ = file.close();
                let _ = raven_rt::fs::chmod(&open_path, 0o600);
                return Ok(display);
            }
            Err(error) => {
                let Some(home) = args.env_get("HOME") else {
                    return Err(error);
                };
                let display = format!("{home}/nohup.out");
                let open_path = format!("file:{display}");
                let Ok(file) = raven_rt::fs::open(&open_path, flags) else {
                    return Err(error);
                };
                let _ = file.close();
                let _ = raven_rt::fs::chmod(&open_path, 0o600);
                Ok(display)
            }
        }
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
        let output = match open_output(&args) {
            Ok(path) => path,
            Err(error) => {
                raven_rt::eprintln!("nohup: cannot create nohup.out: {}", error.description());
                return 1;
            }
        };
        raven_rt::eprintln!("nohup: ignoring input and appending output to '{output}'");
        let environment = raven_env::merge_environment(&base_environment(&args), false, &[]);
        run(&options.command, &environment)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
