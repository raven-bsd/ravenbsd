// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `make` command-line wrapper. The target path spawns programs from `file:`; the host path uses
//! `/bin/sh -c` so the same planner is exercised by the POSIX utility tests.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(not(target_os = "none"))]
fn main() {
    use std::process::Command;

    struct HostFs;

    impl make::FileSystem for HostFs {
        fn exists(&mut self, path: &str) -> bool {
            std::path::Path::new(path).exists()
        }

        fn mtime(&mut self, path: &str) -> Option<i64> {
            std::fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
        }

        fn read(&mut self, path: &str) -> Result<String, String> {
            std::fs::read_to_string(path).map_err(|error| error.to_string())
        }
    }

    struct ShellRunner;

    impl make::Runner for ShellRunner {
        fn run(&mut self, line: &str) -> Result<(), String> {
            let status = Command::new("/bin/sh")
                .arg("-c")
                .arg(line)
                .status()
                .map_err(|error| error.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("exit status {status}"))
            }
        }
    }

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let (config, goals) = match make::parse_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("make: {error}");
            std::process::exit(2);
        }
    };
    let mut fs = HostFs;
    let mut runner = ShellRunner;
    match make::run(&config, &goals, &mut fs, &mut runner, &mut |line| {
        println!("{line}");
    }) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("make: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_proto_fs::OpenFlags;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnOptions, spawn_file};
    use raven_scheme::Client;

    entry!(main);

    struct TargetFs;

    impl make::FileSystem for TargetFs {
        fn exists(&mut self, path: &str) -> bool {
            fs::stat(&scheme(path)).is_ok()
        }

        fn mtime(&mut self, path: &str) -> Option<i64> {
            fs::stat(&scheme(path)).ok().map(|stat| stat.mtime)
        }

        fn read(&mut self, path: &str) -> Result<String, String> {
            let data =
                fs::read_to_vec(&scheme(path)).map_err(|error| error.description().to_string())?;
            String::from_utf8(data).map_err(|_| "makefile is not UTF-8".to_string())
        }
    }

    struct SpawnRunner;

    impl make::Runner for SpawnRunner {
        fn run(&mut self, line: &str) -> Result<(), String> {
            if line
                .chars()
                .any(|character| matches!(character, '>' | '<' | '|' | ';' | '&' | '\'' | '"'))
            {
                return Err("shell metacharacters are unsupported on target".to_string());
            }
            let mut words = line.split_whitespace();
            let program = words.next().ok_or_else(|| "empty recipe".to_string())?;
            let args: Vec<&str> = words.collect();
            let path = if program.starts_with('/') {
                program.to_string()
            } else {
                let mut found = String::new();
                for directory in ["/bin", "/sbin"] {
                    let candidate = format!("{directory}/{program}");
                    if fs::stat(&scheme(&candidate)).is_ok() {
                        found = candidate;
                        break;
                    }
                }
                if found.is_empty() {
                    return Err(format!("{program}: command not found"));
                }
                found
            };
            let client = Client::new();
            let handle = client
                .open_flags(&scheme(&path), OpenFlags::READ)
                .map_err(|error| error.description().to_string())?;
            let options = SpawnOptions {
                args: &args,
                env: &[],
            };
            let process = spawn_file(handle, &path, &options, false)
                .map_err(|error| format!("spawn {path}: {error:?}"))?;
            let _ = client.close(handle);
            let mut code = 0i32;
            let waited = RtSys.proc_wait(process, &mut code);
            let _ = raven_rt::handle_close(process);
            match waited {
                Ok(()) if code == 0 => Ok(()),
                Ok(()) => Err(format!("exit status {code}")),
                Err(error) => Err(error.description().to_string()),
            }
        }
    }

    fn scheme(path: &str) -> String {
        if path.contains(':') {
            path.to_string()
        } else {
            format!("file:{}", fs::resolve(path))
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let args = args.iter().skip(1).map(str::to_string).collect::<Vec<_>>();
        let (config, goals) = match make::parse_args(args) {
            Ok(parsed) => parsed,
            Err(error) => {
                raven_rt::eprintln!("make: {error}");
                return 2;
            }
        };
        let mut fs = TargetFs;
        let mut runner = SpawnRunner;
        match make::run(&config, &goals, &mut fs, &mut runner, &mut |line| {
            raven_rt::println!("{line}");
        }) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("make: {error}");
                1
            }
        }
    }
}
