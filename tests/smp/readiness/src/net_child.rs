// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K543/S36 strict namespace child.
//!
//! - `before`: forked before `net`/`file` registration; retries `ENOENT`, then must see
//!   `EACCES` for both names because its namespace snapshot predates them.
//! - `after`: forked after registration; must resolve both names and complete net bind/listen.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_proto_fs::OpenFlags;
use raven_proto_net::socket::{Call, Endpoint, SocketRequest};
use raven_rt::{entry, handle_close, nanosleep, println};
use raven_scheme::Client;

entry!(main);

/// Retry `ENOENT` while the parent's publishers register.
const ATTEMPTS: u32 = 200;
/// Retry delay while the publishers are not yet registered.
const RETRY_NS: u64 = 10_000_000;

fn label(result: &Option<Result<(), Errno>>) -> &'static str {
    match result {
        None => "timeout",
        Some(Ok(())) => "ok",
        Some(Err(Errno::EACCES)) => "EACCES",
        Some(Err(_)) => "error",
    }
}

fn open_once(client: &Client, path: &str, flags: u32, slot: &mut Option<Result<(), Errno>>) {
    if slot.is_some() {
        return;
    }
    match client.open(path, flags) {
        Ok(handle) => {
            let _ = handle_close(handle);
            *slot = Some(Ok(()));
        }
        Err(Errno::ENOENT) => {}
        Err(error) => *slot = Some(Err(error)),
    }
}

fn before(client: &Client, flags: u32) -> i32 {
    let mut net = None;
    let mut file = None;
    for _ in 0..ATTEMPTS {
        open_once(client, "net:tcp/0.0.0.0:2222", flags, &mut net);
        open_once(client, "file:/x", flags, &mut file);
        if net.is_some() && file.is_some() {
            break;
        }
        let _ = nanosleep(RETRY_NS);
    }
    let net_label = label(&net);
    let file_label = label(&file);
    println!("net-child: before net={} file={}", net_label, file_label);
    if net_label == "EACCES" && file_label == "EACCES" {
        0
    } else {
        1
    }
}

fn after(client: &Client, flags: u32) -> i32 {
    let net = match client.open("net:tcp/0.0.0.0:2222", flags) {
        Ok(handle) => handle,
        Err(error) => {
            println!("net-child: after net={}", error.0);
            return 1;
        }
    };
    let file = match client.open("file:/x", flags) {
        Ok(handle) => handle,
        Err(error) => {
            println!("net-child: after file={}", error.0);
            return 1;
        }
    };
    let _ = handle_close(file);

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
    if client.call(net, &request[..length], &mut response).is_err() {
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
    if client.call(net, &request[..length], &mut response).is_err() {
        println!("net-child: listen error");
        return 1;
    }

    println!("net-child: after net=ok file=ok bind=ok listen=ok");
    0
}

fn main(args: raven_rt::Args) -> i32 {
    let client = Client::new();
    let flags = u32::from(OpenFlags::READ_WRITE.to_wire());
    match args.get(0) {
        Some("before") => before(&client, flags),
        Some("after") => after(&client, flags),
        _ => {
            println!("net-child: mode");
            1
        }
    }
}
