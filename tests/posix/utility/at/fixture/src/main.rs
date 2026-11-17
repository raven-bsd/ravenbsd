// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P40 atd qtest fixture: start the VFS stack, queue one regular and one batch job, run atd.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};

use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(message) => {
            println!("at: fail {message}");
            1
        }
    }
}

fn run() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map".to_string())?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse".to_string())?;
    for (path, scheme) in [
        ("/sbin/initfsd", "initfs:"),
        ("/sbin/ramfsd", "ramfs:"),
        ("/sbin/vfsd", "file:"),
    ] {
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| "spawn service".to_string())?;
        wait_scheme(scheme)?;
    }
    println!("at: services ok");

    fs::mount("/var/at", "ramfs:").map_err(|_| "mount queue".to_string())?;
    let mut spool = at::TargetSpool;
    let now = at::now_seconds();
    let regular = at::submit(&mut spool, now, "now", "echo at-ok", false)
        .map_err(|error| format!("queue regular job: {error}"))?;
    let batch = at::submit(&mut spool, now, "now", "echo batch-ok", true)
        .map_err(|error| format!("queue batch job: {error}"))?;
    println!("at: queue ok regular={} batch={}", regular.id, batch.id);

    spawn(
        &initfs,
        "/sbin/atd",
        &SpawnOptions {
            args: &[],
            env: &[],
        },
    )
    .map_err(|_| "spawn atd".to_string())?;
    let _ = raven_rt::nanosleep(8_000_000_000);
    println!("at: fixture ok");
    Ok(())
}

fn wait_scheme(scheme: &str) -> Result<(), String> {
    for _ in 0..3000 {
        if fs::open(scheme, raven_proto_fs::O_RDONLY).is_ok() {
            return Ok(());
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
    Err("scheme timeout".to_string())
}
