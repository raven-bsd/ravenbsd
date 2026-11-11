// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S32 controller: cooperative server, canceller, then the two-thread capture client.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_s32_capture::{O_RDWR, call, close, map_initfs_image, open, proc_wait};

entry!(main);

const SERVER: &str = "/server";
const CLIENT: &str = "/client";
const CANCELLER: &str = "/canceller";
const RETRIES: usize = 2000;

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = raven_s32_capture::nanosleep(1_000_000);
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

fn wait_ok(child: Handle) -> bool {
    matches!(proc_wait(child), Ok(0))
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => {
            println!("s32: initfs map failed");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("s32: initfs parse failed");
            return 1;
        }
    };
    let server = match spawn_child(&initfs, SERVER) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s32: server spawn failed");
            return 1;
        }
    };
    let control = match open_retry("s32:echo") {
        Ok(handle) => handle,
        Err(_) => {
            println!("s32: server did not register");
            return 1;
        }
    };

    let canceller = match spawn_child(&initfs, CANCELLER) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s32: canceller spawn failed");
            return 1;
        }
    };
    if !wait_ok(canceller) {
        println!("s32: canceller child failed");
        return 1;
    }
    if call(control, b"alive", None).is_err() {
        println!("s32: server did not survive the cancellation");
        return 1;
    }
    println!("s32: cancel ok");

    let client = match spawn_child(&initfs, CLIENT) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s32: client spawn failed");
            return 1;
        }
    };
    if !wait_ok(client) {
        println!("s32: capture client failed");
        return 1;
    }
    let _ = proc_wait(server);
    let _ = close(control);
    println!("s32: lifecycle ok");
    0
}
