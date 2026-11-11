// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S33 server-death client: block on a server that receives without replying, then exits.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::println;
use raven_s33_ipc::{O_RDWR, call, open};

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    let file = match open("s33:block", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: block open failed");
            return 1;
        }
    };
    match call(file, b"block", None) {
        Err(Errno::EIO) => {
            println!("s33: server death ok");
            0
        }
        _ => {
            println!("s33: server death did not wake the client");
            1
        }
    }
}
