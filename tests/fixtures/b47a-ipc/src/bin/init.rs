// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B47a `/init`: boot real `svcd`, then run the client/server IPC scenarios on aarch64.

#![no_std]
#![no_main]

use raven_b47a_ipc::{
    CLIENT_PATH, SERVER_PATH, SVCD_PATH, map_initfs, open_retry, spawn_child, wait,
};
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Client;

entry!(main);

const RETRIES: usize = 200_000;

fn svcd_ready() -> bool {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match raven_rt::fs::open("svc:/b47a", 0) {
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

fn main(_args: raven_rt::Args) -> i32 {
    let Some(initfs) = map_initfs() else {
        println!("b47a: initfs map failed");
        return 1;
    };

    let svcd = match spawn_child(&initfs, SVCD_PATH, &[]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47a: svcd spawn failed");
            return 1;
        }
    };
    println!("b47a: svcd spawned");
    if !svcd_ready() {
        println!("b47a: svcd svc: failed");
        return 1;
    }
    println!("b47a: svcd svc: ok");
    let _ = raven_rt::proc_terminate(svcd, 0);
    let _ = wait(svcd);

    let server = match spawn_child(&initfs, SERVER_PATH, &[]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47a: server spawn failed");
            return 1;
        }
    };
    let ctl = match open_retry("b47a:ctl", 1) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47a: control open failed");
            return 1;
        }
    };
    println!("b47a: server ready");

    let client = match spawn_child(&initfs, CLIENT_PATH, &[]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47a: client spawn failed");
            return 1;
        }
    };
    if wait(client).unwrap_or(-1) != 0 {
        println!("b47a: client failed");
        return 1;
    }
    println!("b47a: roundtrip ok");

    let death = match spawn_child(&initfs, CLIENT_PATH, &["death"]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47a: death spawn failed");
            return 1;
        }
    };
    let client_api = Client::new();
    let mut state = [0u8; 16];
    let mut held = false;
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match client_api.call(ctl, b"state", &mut state) {
            Ok(length) if length >= 4 && &state[..4] == b"held" => {
                held = true;
                break;
            }
            _ => {
                core::hint::spin_loop();
                attempt = attempt.saturating_add(1);
            }
        }
    }
    if !held {
        println!("b47a: client did not hold");
        return 1;
    }
    println!("b47a: client held");
    let _ = raven_rt::proc_terminate(death, -9);
    let _ = wait(death);

    let mut released = [0u8; 16];
    let length = match client_api.call(ctl, b"release", &mut released) {
        Ok(length) => length,
        Err(_) => {
            println!("b47a: release call failed");
            return 1;
        }
    };
    if length != 8 || &released[..8] != b"released" {
        println!("b47a: release failed");
        return 1;
    }
    println!("b47a: release ok");

    let _ = client_api.close(ctl);
    let _ = raven_rt::proc_terminate(server, 0);
    let _ = wait(server);
    println!("b47a: done");
    0
}
