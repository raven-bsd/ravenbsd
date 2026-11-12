// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S39 aarch64 parity controller: real svcd, namespace prefix grant, event registration in a
//! client, client death cleanup and server survival.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_s39_parity::{
    CLIENT_PATH, SERVER_PATH, SVCD_PATH, map_initfs, namespace_add, namespace_clear, namespace_op,
    namespace_remove, open_retry, spawn_child, wait,
};
use raven_scheme::Client;

entry!(main);

const RETRIES: usize = 200_000;

fn fail(message: &str) -> i32 {
    println!("s39: failed {message}");
    1
}

fn svcd_ready() -> bool {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match raven_rt::fs::open("svc:/s39", 0) {
            Ok(mut file) => {
                let mut line = [0u8; 128];
                let read = file.read(&mut line).unwrap_or(0);
                let _ = file.close();
                if read > 0 {
                    return true;
                }
            }
            Err(_) => {}
        }
        core::hint::spin_loop();
        attempt = attempt.saturating_add(1);
    }
    false
}

fn namespace_self_test() -> bool {
    if namespace_add(Handle::SELF_PROC, "s39tmp").is_err() {
        return false;
    }
    if namespace_remove(Handle::SELF_PROC, "s39tmp").is_err() {
        return false;
    }
    if namespace_add(Handle::SELF_PROC, "s39tmp").is_err() {
        return false;
    }
    if namespace_clear(Handle::SELF_PROC).is_err() {
        return false;
    }
    if !matches!(
        namespace_op(Handle::SELF_PROC, 99, "s39tmp"),
        Err(Errno::EINVAL)
    ) {
        return false;
    }
    if !matches!(namespace_add(Handle::SELF_PROC, "Bad"), Err(Errno::EINVAL)) {
        return false;
    }
    if !matches!(namespace_add(Handle::NONE, "s39tmp"), Err(Errno::EBADF)) {
        return false;
    }
    true
}

fn main(_args: raven_rt::Args) -> i32 {
    let Some(initfs) = map_initfs() else {
        return fail("initfs map");
    };

    let svcd = match spawn_child(&initfs, SVCD_PATH, &[]) {
        Ok(handle) => handle,
        Err(_) => return fail("svcd spawn"),
    };
    if !svcd_ready() {
        return fail("svcd svc");
    }
    println!("s39: svcd svc: ok");
    let _ = raven_rt::proc_terminate(svcd, 0);
    let _ = wait(svcd);

    if !namespace_self_test() {
        return fail("namespace syscall");
    }
    println!("s39: namespace syscall ok");

    // Spawn the client before the fixture server registers, so only the granted prefix (not the
    // spawn-time name snapshot) can make the granted scheme visible.
    let client = match spawn_child(&initfs, CLIENT_PATH, &[]) {
        Ok(handle) => handle,
        Err(_) => return fail("client spawn"),
    };
    if namespace_add(client, "b47a").is_err() {
        return fail("prefix grant");
    }
    println!("s39: prefix granted");

    let server = match spawn_child(&initfs, SERVER_PATH, &[]) {
        Ok(handle) => handle,
        Err(_) => return fail("server spawn"),
    };
    let control = match open_retry("b47a:ctl", 1) {
        Ok(handle) => handle,
        Err(_) => return fail("server open"),
    };
    println!("s39: server ready");

    let api = Client::new();
    let mut state = [0u8; 16];
    let mut armed = false;
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match api.call(control, b"state", &mut state) {
            Ok(length) if length >= 5 && &state[..5] == b"armed" => {
                armed = true;
                break;
            }
            Ok(_) | Err(Errno::EAGAIN) | Err(Errno::EIO) => {
                core::hint::spin_loop();
                attempt = attempt.saturating_add(1);
            }
            Err(_) => return fail("state call"),
        }
    }
    if !armed {
        let _ = raven_rt::proc_terminate(client, -9);
        let _ = wait(client);
        return fail("client did not arm");
    }
    println!("s39: client armed");

    let _ = raven_rt::proc_terminate(client, -9);
    let _ = wait(client);
    println!("s39: client killed");

    let mut pong = [0u8; 8];
    match api.call(control, b"ping", &mut pong) {
        Ok(length) if length == 4 && &pong[..4] == b"pong" => println!("s39: server survives"),
        _ => return fail("server ping"),
    }

    let _ = api.close(control);
    let _ = raven_rt::proc_terminate(server, 0);
    let _ = wait(server);
    println!("s39: done");
    0
}
