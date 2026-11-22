// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K543 net listener child: open the `net` scheme from a pre-net namespace snapshot and issue
//! `Bind`/`Listen`. The child is spawned before the parent registers the minimal `net` publisher;
//! the kernel must still allow a root child to see the protected boot service.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_proto_fs::OpenFlags;
use raven_proto_net::socket::{Call, Endpoint, SocketRequest};
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Client;

entry!(main);

/// Retry `ENOENT` while the parent's publisher registers; `EACCES` is terminal.
const ATTEMPTS: u32 = 200;
/// Retry delay while the publisher is not yet registered.
const RETRY_NS: u64 = 10_000_000;

fn main(_args: raven_rt::Args) -> i32 {
    let client = Client::new();
    let flags = u32::from(OpenFlags::READ_WRITE.to_wire());
    let mut file = None;
    let mut attempt = 0u32;
    while attempt < ATTEMPTS {
        match client.open("net:tcp/0.0.0.0:2222", flags) {
            Ok(handle) => {
                file = Some(handle);
                break;
            }
            Err(Errno::ENOENT) => {
                let _ = raven_rt::nanosleep(RETRY_NS);
            }
            Err(error) => {
                println!("net-child: open={}", error.0);
                return 1;
            }
        }
        attempt = attempt.saturating_add(1);
    }
    let Some(file) = file else {
        println!("net-child: open=timeout");
        return 1;
    };

    let mut request = [0u8; 64];
    let mut response = [0u8; 64];
    let bind = SocketRequest::Call(Call::Bind(Endpoint {
        ip: [0, 0, 0, 0],
        port: 2222,
    }));
    let length = match bind.encode(&mut request) {
        Ok(length) => length,
        Err(_) => {
            println!("net-child: bind encode");
            return 1;
        }
    };
    if client
        .call(file, &request[..length], &mut response)
        .is_err()
    {
        println!("net-child: bind error");
        return 1;
    }

    let listen = SocketRequest::Call(Call::Listen { backlog: 4 });
    let length = match listen.encode(&mut request) {
        Ok(length) => length,
        Err(_) => {
            println!("net-child: listen encode");
            return 1;
        }
    };
    if client
        .call(file, &request[..length], &mut response)
        .is_err()
    {
        println!("net-child: listen error");
        return 1;
    }

    println!("net-child: bind=ok listen=ok");
    0
}
