// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `at` command-line wrapper.

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

    fn read_submission(command: Option<String>, file: Option<String>) -> Result<String, String> {
        if let Some(command) = command {
            return Ok(command);
        }
        if let Some(file) = file {
            if file == "-" {
                let mut text = String::new();
                std::io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|error| format!("stdin: {error}"))?;
                return Ok(text);
            }
            return std::fs::read_to_string(&file).map_err(|error| format!("{file}: {error}"));
        }
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| format!("stdin: {error}"))?;
        Ok(text)
    }

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let action = match at::parse_at_args(&args) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("at: {error}");
            return 2;
        }
    };
    let directory = at::spool_dir();
    let mut spool = at::HostSpool::new(&directory);
    match action {
        at::AtAction::List => match at::list(&mut spool) {
            Ok(jobs) => {
                for job in jobs {
                    println!("{} {} {}", job.id, job.due, job.command);
                }
                0
            }
            Err(error) => {
                eprintln!("at: {error}");
                1
            }
        },
        at::AtAction::Remove(ids) => {
            let mut code = 0;
            for id in ids {
                if let Err(error) = at::remove(&mut spool, &id) {
                    eprintln!("at: {error}");
                    code = 1;
                }
            }
            code
        }
        at::AtAction::Schedule {
            when,
            command,
            file,
        } => {
            let text = match read_submission(command, file) {
                Ok(text) => text,
                Err(error) => {
                    eprintln!("at: {error}");
                    return 1;
                }
            };
            match at::submit(&mut spool, at::now_seconds(), &when, &text, false) {
                Ok(job) => {
                    println!("at: scheduled {}", job.id);
                    0
                }
                Err(error) => {
                    eprintln!("at: {error}");
                    1
                }
            }
        }
    }
}

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use at::{AtAction, TargetSpool};
    use raven_rt::entry;
    use raven_rt::fs;

    entry!(main);

    fn read_submission(command: Option<String>, file: Option<String>) -> Result<String, String> {
        if let Some(command) = command {
            return Ok(command);
        }
        let file = file.ok_or_else(|| {
            "standard input is unsupported on target; pass a command or -f file".to_string()
        })?;
        let data = fs::read_to_vec(&scheme(&file))
            .map_err(|error| format!("{file}: {}", error.description()))?;
        String::from_utf8(data).map_err(|_| format!("{file}: not UTF-8"))
    }

    fn main(args: raven_rt::Args) -> i32 {
        let args = args.iter().skip(1).map(str::to_string).collect::<Vec<_>>();
        let action = match at::parse_at_args(&args) {
            Ok(action) => action,
            Err(error) => {
                raven_rt::eprintln!("at: {error}");
                return 2;
            }
        };
        let mut spool = TargetSpool;
        match action {
            AtAction::List => match at::list(&mut spool) {
                Ok(jobs) => {
                    for job in jobs {
                        raven_rt::println!("{} {} {}", job.id, job.due, job.command);
                    }
                    0
                }
                Err(error) => {
                    raven_rt::eprintln!("at: {error}");
                    1
                }
            },
            AtAction::Remove(ids) => {
                let mut code = 0;
                for id in ids {
                    if let Err(error) = at::remove(&mut spool, &id) {
                        raven_rt::eprintln!("at: {error}");
                        code = 1;
                    }
                }
                code
            }
            AtAction::Schedule {
                when,
                command,
                file,
            } => {
                let text = match read_submission(command, file) {
                    Ok(text) => text,
                    Err(error) => {
                        raven_rt::eprintln!("at: {error}");
                        return 1;
                    }
                };
                match at::submit(&mut spool, at::now_seconds(), &when, &text, false) {
                    Ok(job) => {
                        raven_rt::println!("at: scheduled {}", job.id);
                        0
                    }
                    Err(error) => {
                        raven_rt::eprintln!("at: {error}");
                        1
                    }
                }
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
}
