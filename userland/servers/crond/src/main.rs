// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal `crond`: read `/var/cron/<user>` spools, match minute fields, and run each command
//! through the kernel spawn path. It never owns pids, wait, signals or credentials.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(not(target_os = "none"))]
fn main() {
    eprintln!("crond: target-only service");
    std::process::exit(1);
}

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use crontab::{Civil, Entry, civil_from_unix, entry_matches, parse, spool_dir};
    use raven_proto_fs::{DT_REG, OpenFlags};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnOptions, spawn_file};
    use raven_scheme::Client;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        let mut last_minute = None;
        raven_rt::println!("crond: ready spool={}", spool_dir());
        loop {
            let now = match raven_rt::clock_get(0) {
                Ok(time) => time.tv_sec,
                Err(error) => {
                    raven_rt::eprintln!("crond: clock: {}", error.description());
                    let _ = raven_rt::nanosleep(1_000_000_000);
                    continue;
                }
            };
            let minute = now.div_euclid(60);
            if last_minute != Some(minute) {
                last_minute = Some(minute);
                let civil = civil_from_unix(now);
                run_minute(&civil);
            }
            let _ = raven_rt::nanosleep(1_000_000_000);
        }
    }

    fn run_minute(civil: &Civil) {
        let directory = spool_dir();
        let path = scheme(&directory);
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => {
                raven_rt::println!("crond: spool unavailable: {}", error.description());
                return;
            }
        };
        for entry in entries {
            if entry.kind != DT_REG {
                continue;
            }
            let user = entry.name;
            let spool = format!("{directory}/{user}");
            let text = match fs::read_to_vec(&scheme(&spool)) {
                Ok(data) => match String::from_utf8(data) {
                    Ok(text) => text,
                    Err(_) => {
                        raven_rt::println!("crond: {user}: not UTF-8");
                        continue;
                    }
                },
                Err(error) => {
                    raven_rt::println!("crond: {user}: {}", error.description());
                    continue;
                }
            };
            let parsed = match parse(&text) {
                Ok(parsed) => parsed,
                Err(error) => {
                    raven_rt::println!("crond: {user}: {error}");
                    continue;
                }
            };
            for job in parsed {
                if entry_matches(
                    &job,
                    civil.minute,
                    civil.hour,
                    civil.day,
                    civil.month,
                    civil.weekday,
                ) {
                    run_job(&user, &job);
                }
            }
        }
    }

    fn run_job(user: &str, job: &Entry) {
        raven_rt::println!("crond: start user={user} command={}", job.command);
        let status = spawn_command(&job.command);
        match status {
            Ok(code) => raven_rt::println!("crond: exit user={user} status={code}"),
            Err(error) => raven_rt::println!("crond: exit user={user} error={error}"),
        }
    }

    fn spawn_command(command: &str) -> Result<i32, String> {
        if command
            .chars()
            .any(|character| matches!(character, '>' | '<' | '|' | ';' | '&' | '\'' | '"'))
        {
            return Err("shell metacharacters are unsupported".to_string());
        }
        let mut words = command.split_whitespace();
        let program = words.next().ok_or_else(|| "empty command".to_string())?;
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
            Ok(()) => Ok(code),
            Err(error) => Err(error.description().to_string()),
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
