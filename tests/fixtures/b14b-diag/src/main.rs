// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B14b fixture: start netd, run ping, route and netstat over the loopback `net:` API.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::String;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_proto_net::socket::{Call, Endpoint, Response, SocketRequest};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

const NETD_PATH: &str = "/sbin/netd";
const PING_PATH: &str = "/bin/ping";
const ROUTE_PATH: &str = "/bin/route";
const NETSTAT_PATH: &str = "/bin/netstat";
const RETRIES: usize = 200_000;
const READ_WRITE: u32 = 3;

fn map_initfs() -> Option<Initfs<'static>> {
    let image = raven_rt::map_initfs().ok()?;
    Initfs::parse(image).ok()
}

fn spawn_child(initfs: &Initfs<'_>, path: &str, args: &[&str]) -> Result<Handle, Errno> {
    spawn(initfs, path, &SpawnOptions { args, env: &[] }).map_err(|_| Errno::EIO)
}

fn wait(process: Handle) -> Result<i32, Errno> {
    let mut code = 0i32;
    // SAFETY: PROC_WAIT writes one i32 to this live stack slot and validates the handle.
    let raw = unsafe {
        raven_rt::sys::syscall2(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            &mut code as *mut i32 as usize,
        )
    };
    Errno::from_ret(raw).map(|_| code)
}

fn wait_netd() -> bool {
    let client = Client::new();
    let mut attempt = 0usize;
    while attempt < RETRIES {
        if let Ok(handle) = client.open("net:", 1) {
            let _ = client.close(handle);
            return true;
        }
        core::hint::spin_loop();
        attempt += 1;
    }
    false
}

fn run(initfs: &Initfs<'_>, path: &str, args: &[&str]) -> bool {
    let Ok(child) = spawn_child(initfs, path, args) else {
        return false;
    };
    matches!(wait(child), Ok(0))
}

fn bind_udp(client: &Client, port: u16) -> Result<Handle, String> {
    let path = format!("net:udp/127.0.0.1:{port}");
    let handle = client
        .open(&path, READ_WRITE)
        .map_err(|error| format!("udp open: {}", error.description()))?;
    let request = SocketRequest::Call(Call::Bind(Endpoint {
        ip: [127, 0, 0, 1],
        port,
    }));
    let mut encoded = [0u8; 32];
    let length = request
        .encode(&mut encoded)
        .map_err(|_| String::from("udp bind encode"))?;
    let mut response = [0u8; 64];
    let read = client
        .call(handle, &encoded[..length], &mut response)
        .map_err(|error| format!("udp bind: {}", error.description()))?;
    match Response::decode(response.get(..read).ok_or("short udp bind response")?) {
        Ok(Response::Ok) => Ok(handle),
        Ok(Response::Err(error)) => Err(format!("udp bind: {}", error.description())),
        _ => Err(String::from("unexpected udp bind response")),
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let Some(initfs) = map_initfs() else {
        println!("net-diag: initfs map failed");
        return 1;
    };
    let netd = match spawn_child(&initfs, NETD_PATH, &[]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("net-diag: netd spawn failed");
            return 1;
        }
    };
    if !wait_netd() {
        println!("net-diag: netd did not register");
        return 1;
    }
    println!("net-diag: netd ready");

    if !run(
        &initfs,
        PING_PATH,
        &["ping", "-c", "2", "-w", "3", "127.0.0.1"],
    ) {
        println!("net-diag: ping failed");
        return 1;
    }
    println!("net-diag: ping ok");

    if !run(&initfs, ROUTE_PATH, &["route"]) {
        println!("net-diag: route list failed");
        return 1;
    }
    println!("net-diag: route list ok");

    if !run(
        &initfs,
        ROUTE_PATH,
        &["route", "add", "host", "198.51.100.10", "lo"],
    ) {
        println!("net-diag: route add failed");
        return 1;
    }
    println!("net-diag: route add ok");

    if !run(&initfs, ROUTE_PATH, &["route"]) {
        println!("net-diag: route added list failed");
        return 1;
    }
    println!("net-diag: route added list ok");

    if !run(
        &initfs,
        ROUTE_PATH,
        &["route", "delete", "host", "198.51.100.10", "lo"],
    ) {
        println!("net-diag: route delete failed");
        return 1;
    }
    println!("net-diag: route delete ok");

    let client = Client::new();
    let udp = match bind_udp(&client, 7102) {
        Ok(handle) => handle,
        Err(error) => {
            println!("net-diag: {error}");
            return 1;
        }
    };
    if !run(&initfs, NETSTAT_PATH, &["netstat"]) {
        println!("net-diag: netstat failed");
        return 1;
    }
    println!("net-diag: netstat ok");
    let _ = client.close(udp);

    let _ = raven_rt::proc_terminate(netd, 0);
    let _ = wait(netd);
    println!("net-diag: done");
    0
}
