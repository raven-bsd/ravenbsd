// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal `atd`: scan `/var/at` once per second, run due jobs through the kernel spawn path and
//! remove them. It never owns pids, wait, signals or credentials; a restart resumes from the
//! spool.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(not(target_os = "none"))]
fn main() {
    eprintln!("atd: target-only service");
    std::process::exit(1);
}

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use at::{Job, TargetSpool, due_queues};
    use raven_proto_fs::OpenFlags;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnOptions, spawn_file};
    use raven_scheme::Client;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        let mut spool = TargetSpool;
        raven_rt::println!("atd: ready spool={}", at::spool_dir());
        loop {
            let now = match raven_rt::clock_get(0) {
                Ok(time) => time.tv_sec,
                Err(error) => {
                    raven_rt::eprintln!("atd: clock: {}", error.description());
                    let _ = raven_rt::nanosleep(1_000_000_000);
                    continue;
                }
            };
            let jobs = match at::list(&mut spool) {
                Ok(jobs) => jobs,
                Err(error) => {
                    raven_rt::println!("atd: queue unavailable: {error}");
                    let _ = raven_rt::nanosleep(1_000_000_000);
                    continue;
                }
            };
            let (regular, batch) = due_queues(&jobs, now);
            for job in &regular {
                run_job(&mut spool, job);
            }
            // Batch runs only on a tick with no due regular job. There is no load-average
            // source in the kernel, so an idle scheduler tick is the low-load proxy.
            if regular.is_empty() {
                for job in &batch {
                    run_job(&mut spool, job);
                }
            }
            let _ = raven_rt::nanosleep(1_000_000_000);
        }
    }

    fn run_job(spool: &mut TargetSpool, job: &Job) {
        raven_rt::println!("atd: start job={} command={}", job.id, job.command);
        let status = spawn_command(&job.command);
        match status {
            Ok(code) => raven_rt::println!("atd: exit job={} status={code}", job.id),
            Err(error) => raven_rt::println!("atd: exit job={} error={error}", job.id),
        }
        if let Err(error) = at::remove(spool, &job.id) {
            raven_rt::println!("atd: remove job={} error={error}", job.id);
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
