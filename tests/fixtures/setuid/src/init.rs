// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! U3b setuid/setgid exec fixture.
//!
//! Starts `initfsd`, opens helper images from the initfs, and spawns them with the native exec
//! path. The helper prints the credentials the kernel installed. The fixture checks the 0644
//! denial before dropping to uid/gid 1000, then the 0755, 4755, 2755, script and `nosuid` cases.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;

use raven_abi::errno::Errno;
use raven_initfs::Initfs;
use raven_proto_fs::OpenFlags;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnError, SpawnOptions, spawn, spawn_file};
use raven_scheme::Client;
use raven_setuid::{setcred, wait};

entry!(main);

const INITFSD: &str = "/sbin/initfsd";

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(step) = run() {
        println!("setuid: fail {step}");
        return 1;
    }
    println!("setuid: ok");
    0
}

fn run() -> Result<(), &'static str> {
    start_initfsd()?;

    match spawn_once("/bin/child-0644", "0644", false) {
        Err(SpawnError::Sys(Errno::EACCES)) => println!("setuid: 0644=denied"),
        Ok(child) => {
            let _ = wait(child);
            return Err("0644 executed");
        }
        Err(_) => return Err("0644 error"),
    }

    setcred(5, 1000).map_err(|_| "setgid 1000")?;
    setcred(1, 1000).map_err(|_| "setuid 1000")?;

    run_child("/bin/child", "0755", false)?;
    run_child("/bin/child-4755", "4755", false)?;
    run_child("/bin/child-2755", "2755", false)?;

    match spawn_once("/bin/child-script", "script", false) {
        Err(SpawnError::BadElf(_)) => println!("setuid: script=bad-elf"),
        _ => return Err("script setuid"),
    }

    run_child("/bin/child-4755", "nosuid", true)?;
    Ok(())
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

fn run_child(path: &str, label: &str, nosuid: bool) -> Result<(), &'static str> {
    println!("setuid: run {label}");
    let child = spawn_once(path, label, nosuid).map_err(|_| "spawn child")?;
    match wait(child) {
        Ok(0) => Ok(()),
        _ => Err("child wait"),
    }
}

fn spawn_once(
    path: &str,
    label: &str,
    nosuid: bool,
) -> Result<raven_abi::types::Handle, SpawnError> {
    let client = Client::new();
    let open_path = format!("initfs:{path}");
    let handle = client
        .open(&open_path, u32::from(OpenFlags::READ.to_wire()))
        .map_err(SpawnError::Sys)?;
    let args = [path, label];
    let options = SpawnOptions {
        args: &args,
        env: &[],
    };
    let child = spawn_file(handle, path, &options, nosuid);
    let _ = client.close(handle);
    child
}
