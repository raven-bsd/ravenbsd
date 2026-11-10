// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S11 echo client: a 1 MiB write/read round trip and a namespace negative.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::println;
use raven_s11_echo::{O_RDWR, close, map_anonymous, open, read, write};

entry!(main);

const TRANSFER: usize = 1024 * 1024;

fn main(_args: raven_rt::Args) -> i32 {
    let handle = match open("echo:hello", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => {
            println!("scheme-echo: client open failed");
            return 1;
        }
    };
    let Some(buffer) = map_anonymous(TRANSFER).ok() else {
        println!("scheme-echo: client map failed");
        return 1;
    };
    // SAFETY: `buffer` is a private 1 MiB anonymous mapping in this process.
    let bytes = unsafe { core::slice::from_raw_parts_mut(buffer, TRANSFER) };
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index as u8) ^ 0x5a;
    }
    if write(handle, bytes).ok() != Some(TRANSFER) {
        println!("scheme-echo: client write failed");
        return 1;
    }
    bytes.fill(0);
    if read(handle, bytes).ok() != Some(TRANSFER) {
        println!("scheme-echo: client read failed");
        return 1;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != ((index as u8) ^ 0x5a) {
            println!("scheme-echo: client data mismatch");
            return 1;
        }
    }
    if !matches!(open("nosuch:x", O_RDWR), Err(Errno::ENOENT)) {
        println!("scheme-echo: namespace negative failed");
        return 1;
    }
    let _ = close(handle);
    println!("scheme-echo: client ok");
    0
}
