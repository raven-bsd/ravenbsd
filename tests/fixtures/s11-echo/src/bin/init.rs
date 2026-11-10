// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S11 controller: spawn the echo server and client, verify EIO and restart recovery.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_s11_echo::{O_RDWR, call, close, map_initfs_image, open, proc_wait, read, write};

entry!(main);

const SERVER: &str = "/server";
const CLIENT: &str = "/client";
const CONTROL: &str = "echo:test";
const RETRIES: usize = 2000;

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = raven_s11_echo::nanosleep(1_000_000);
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

fn block_until_eio(handle: Handle) -> Result<(), Errno> {
    match call(handle, b"block", None) {
        Err(Errno::EIO) => Ok(()),
        _ => Err(Errno::EIO),
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => {
            println!("scheme-echo: initfs map failed");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("scheme-echo: initfs parse failed");
            return 1;
        }
    };

    let server = match spawn_child(&initfs, SERVER) {
        Ok(handle) => handle,
        Err(_) => {
            println!("scheme-echo: server spawn failed");
            return 1;
        }
    };
    let control = match open_retry(CONTROL) {
        Ok(handle) => handle,
        Err(_) => {
            println!("scheme-echo: server did not register");
            return 1;
        }
    };

    let client = match spawn_child(&initfs, CLIENT) {
        Ok(handle) => handle,
        Err(_) => {
            println!("scheme-echo: client spawn failed");
            return 1;
        }
    };
    match proc_wait(client) {
        Ok(0) => {}
        _ => {
            println!("scheme-echo: client failed");
            return 1;
        }
    }
    println!("scheme-echo: 1 MiB ok");

    if block_until_eio(control).is_err() {
        println!("scheme-echo: pending EIO failed");
        return 1;
    }
    let _ = proc_wait(server);
    match write(control, b"x") {
        Err(Errno::EIO) => {}
        _ => {
            println!("scheme-echo: expected EIO");
            return 1;
        }
    }
    let _ = close(control);

    let _server2 = match spawn_child(&initfs, SERVER) {
        Ok(handle) => handle,
        Err(_) => {
            println!("scheme-echo: restart spawn failed");
            return 1;
        }
    };
    let control2 = match open_retry(CONTROL) {
        Ok(handle) => handle,
        Err(_) => {
            println!("scheme-echo: restart did not register");
            return 1;
        }
    };
    if write(control2, b"recover").ok() != Some(7) {
        println!("scheme-echo: restart write failed");
        return 1;
    }
    let mut buffer = [0u8; 7];
    if read(control2, &mut buffer).ok() != Some(7) || &buffer != b"recover" {
        println!("scheme-echo: restart read failed");
        return 1;
    }
    let _ = close(control2);
    println!("scheme-echo: eio ok");
    0
}
