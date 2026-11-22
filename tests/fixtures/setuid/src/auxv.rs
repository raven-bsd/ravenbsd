// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K526 `auxv-spawn` fixture init: spawn the child once and let it prove two execs get
//! distinct, nonzero `AT_RANDOM` values. The child carries the first value into a same-pid
//! `PROC_EXEC` and prints the comparison after the replacement image starts.

#![no_std]
#![no_main]

extern crate alloc;

use raven_initfs::Initfs;
use raven_proto_fs::OpenFlags;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;
use raven_setuid::wait;

entry!(main);

const INITFSD: &str = "/sbin/initfsd";

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(step) = run() {
        println!("auxv-init: fail {step}");
        return 1;
    }
    println!("auxv-init: ok");
    0
}

fn run() -> Result<(), &'static str> {
    start_initfsd()?;
    let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
    let options = SpawnOptions {
        args: &["/bin/child", "auxv"],
        env: &[],
    };
    let child = spawn(&initfs, "/bin/child", &options).map_err(|_| "spawn child")?;
    match wait(child) {
        Ok(0) => Ok(()),
        _ => Err("child wait"),
    }
}

fn start_initfsd() -> Result<(), &'static str> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
    spawn(
        &initfs,
        INITFSD,
        &SpawnOptions {
            args: &[],
            env: &[],
        },
    )
    .map_err(|_| "spawn initfsd")?;
    let mut attempt = 0usize;
    while attempt < 3000 {
        if Client::new()
            .open("initfs:", u32::from(OpenFlags::READ.to_wire()))
            .is_ok()
        {
            return Ok(());
        }
        let _ = raven_rt::nanosleep(10_000_000);
        attempt = attempt.saturating_add(1);
    }
    Err("initfs timeout")
}
