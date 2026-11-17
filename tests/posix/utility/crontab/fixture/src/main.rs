// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P4y crond qtest fixture: start the VFS stack, install one spool, run crond once.

#![no_std]
#![no_main]

extern crate alloc;

use raven_initfs::Initfs;
use raven_proto_fs::{O_CREAT, O_WRONLY};
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(message) => {
            println!("crontab: fail {message}");
            1
        }
    }
}

fn run() -> Result<(), &'static str> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
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
        .map_err(|_| "spawn service")?;
        wait_scheme(scheme)?;
    }
    println!("crontab: services ok");

    fs::mount("/var/cron", "ramfs:").map_err(|_| "mount spool")?;
    let mut spool =
        fs::open("file:/var/cron/root", O_WRONLY | O_CREAT).map_err(|_| "open spool")?;
    spool
        .write(b"* * * * * echo cron-ok\n")
        .map_err(|_| "write spool")?;
    spool.close().map_err(|_| "close spool")?;
    println!("crontab: spool ok");

    spawn(
        &initfs,
        "/sbin/crond",
        &SpawnOptions {
            args: &[],
            env: &[],
        },
    )
    .map_err(|_| "spawn crond")?;
    let _ = raven_rt::nanosleep(3_000_000_000);
    println!("crontab: fixture ok");
    Ok(())
}

fn wait_scheme(scheme: &str) -> Result<(), &'static str> {
    for _ in 0..3000 {
        if fs::open(scheme, raven_proto_fs::O_RDONLY).is_ok() {
            return Ok(());
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
    Err("scheme timeout")
}
