// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K71 controller/client: opens `k71:test` and checks reply attachments.
//!
//! `attach` returns inline data plus one MemObject pair; `attach-two` returns
//! two pairs; `norights` must fail with EPERM; `plain` keeps the legacy
//! no-attachment CALL path.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

use raven_k71_scheme::{close, first_byte, pair_at};

entry!(main);

const READ: u32 = 1;
const RETRIES: usize = 500;
const ATTACH_RIGHTS: u32 = 5;

fn open_retry(client: &Client) -> Result<raven_abi::types::Handle, Errno> {
    let mut attempt = 0;
    while attempt < RETRIES {
        match client.open("k71:test", READ) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = raven_rt::nanosleep(10_000_000);
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match raven_rt::map_initfs() {
        Ok(image) => image,
        Err(_) => return 1,
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => return 2,
    };
    if spawn(
        &initfs,
        "/server",
        &SpawnOptions {
            args: &[],
            env: &[],
        },
    )
    .is_err()
    {
        return 3;
    }
    let client = Client::new();
    let handle = match open_retry(&client) {
        Ok(handle) => handle,
        Err(_) => return 4,
    };

    let mut response = [0u8; 128];
    let length = match client.call(handle, b"attach", &mut response) {
        Ok(length) => length,
        Err(_) => return 5,
    };
    if length != 10 || &response[..2] != b"ok" {
        return 6;
    }
    let Some((first, mask)) = pair_at(&response[..length], 2) else {
        return 7;
    };
    if mask != ATTACH_RIGHTS || first_byte(first, 0xa5).is_err() {
        return 8;
    }
    close(first);
    println!("k71: attach ok");

    let length = match client.call(handle, b"attach-two", &mut response) {
        Ok(length) => length,
        Err(_) => return 9,
    };
    if length != 16 {
        return 10;
    }
    let Some((first, first_mask)) = pair_at(&response[..length], 0) else {
        return 11;
    };
    let Some((second, second_mask)) = pair_at(&response[..length], 8) else {
        return 12;
    };
    if first_mask != ATTACH_RIGHTS || second_mask != ATTACH_RIGHTS || first == second {
        return 13;
    }
    if first_byte(first, 0xa5).is_err() || first_byte(second, 0x5a).is_err() {
        return 14;
    }
    close(first);
    close(second);
    println!("k71: two ok");

    match client.call(handle, b"norights", &mut response) {
        Err(Errno::EPERM) => {}
        _ => return 15,
    }
    println!("k71: norights ok");

    let mut plain = [0u8; 16];
    let length = match client.call(handle, b"plain", &mut plain) {
        Ok(length) => length,
        Err(_) => return 16,
    };
    if length != 5 || &plain[..length] != b"plain" {
        return 17;
    }
    println!("k71: plain ok");
    0
}
