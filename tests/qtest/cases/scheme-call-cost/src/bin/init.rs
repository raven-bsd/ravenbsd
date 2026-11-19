// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K147 controller: spawn the benchmark server and client, wait for the client.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_s11_echo::{O_RDWR, map_initfs_image, nanosleep, open, proc_wait};

entry!(main);

const RETRIES: usize = 2000;

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn spawn_child(initfs: &Initfs<'_>, path: &str) -> Result<Handle, Errno> {
    let options = SpawnOptions {
        args: &[],
        env: &[],
    };
    spawn(initfs, path, &options).map_err(|_| Errno::EIO)
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => {
            println!("scheme-call-cost: fail initfs map");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("scheme-call-cost: fail initfs parse");
            return 1;
        }
    };
    let _server = match spawn_child(&initfs, "/server") {
        Ok(handle) => handle,
        Err(_) => {
            println!("scheme-call-cost: fail server spawn");
            return 1;
        }
    };
    let _control = match open_retry("bench:test") {
        Ok(handle) => handle,
        Err(_) => {
            println!("scheme-call-cost: fail server register");
            return 1;
        }
    };
    let client = match spawn_child(&initfs, "/client") {
        Ok(handle) => handle,
        Err(_) => {
            println!("scheme-call-cost: fail client spawn");
            return 1;
        }
    };
    match proc_wait(client) {
        Ok(0) => {}
        _ => {
            println!("scheme-call-cost: fail client exit");
            return 1;
        }
    }
    println!("scheme-call-cost: controller ok");
    0
}
