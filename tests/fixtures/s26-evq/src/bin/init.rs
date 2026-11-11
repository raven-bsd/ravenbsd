// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S26 controller: transfers the user events, starts the server/clients/signaler and checks HUP.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_s26_evq::{
    Grant, O_RDWR, event_create, map_initfs_image, open, proc_wait, spawn_process,
};

entry!(main);

const RETRIES: usize = 2000;

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = raven_s26_evq::nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn wait_ok(process: Handle, name: &str) -> bool {
    match proc_wait(process) {
        Ok(0) => true,
        Ok(code) => {
            println!("s26: failed {} exit={}", name, code);
            false
        }
        Err(error) => {
            println!("s26: failed {} errno={}", name, error.0);
            false
        }
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => {
            println!("s26: failed initfs");
            return 50;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("s26: failed initfs parse");
            return 51;
        }
    };
    let user_event = match event_create() {
        Ok(event) => event,
        Err(_) => return 52,
    };
    let ack_event = match event_create() {
        Ok(event) => event,
        Err(_) => return 53,
    };
    let server = match spawn_process(
        &initfs,
        "/server",
        &[
            Grant {
                object: user_event,
                rights: Rights::READ,
            },
            Grant {
                object: ack_event,
                rights: Rights::SIGNAL,
            },
        ],
    ) {
        Ok(server) => server,
        Err(error) => {
            println!("s26: failed server spawn errno={}", error.0);
            return 54;
        }
    };
    // The controller's own open is the first scheme request the server answers.
    let _control = match open_retry("evq:test") {
        Ok(handle) => handle,
        Err(_) => {
            println!("s26: failed open");
            return 55;
        }
    };
    let options = SpawnOptions {
        args: &[],
        env: &[],
    };
    let client0 = match spawn(&initfs, "/client", &options) {
        Ok(client) => client,
        Err(_) => return 56,
    };
    let client1 = match spawn(&initfs, "/client", &options) {
        Ok(client) => client,
        Err(_) => return 57,
    };
    let signaler = match spawn_process(
        &initfs,
        "/signaler",
        &[
            Grant {
                object: user_event,
                rights: Rights::SIGNAL,
            },
            Grant {
                object: ack_event,
                rights: Rights::READ.union(Rights::DUP),
            },
        ],
    ) {
        Ok(signaler) => signaler,
        Err(_) => return 58,
    };
    if !wait_ok(server, "server") {
        return 59;
    }
    if !wait_ok(client0, "client0") {
        return 60;
    }
    if !wait_ok(client1, "client1") {
        return 61;
    }
    if !wait_ok(signaler, "signaler") {
        return 62;
    }
    println!("s26: client0 hup ok");
    println!("s26: client1 hup ok");
    println!("s26: hup ok");
    0
}
