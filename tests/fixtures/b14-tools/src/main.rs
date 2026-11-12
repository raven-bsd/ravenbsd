// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B14a fixture: start netd, run ifconfig and nc over the loopback `net:` API.

#![no_std]
#![no_main]

extern crate alloc;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

const NETD_PATH: &str = "/sbin/netd";
const IFCONFIG_PATH: &str = "/bin/ifconfig";
const NC_PATH: &str = "/bin/nc";
const RETRIES: usize = 200_000;

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

fn main(_args: raven_rt::Args) -> i32 {
    let Some(initfs) = map_initfs() else {
        println!("net-tools: initfs map failed");
        return 1;
    };
    let netd = match spawn_child(&initfs, NETD_PATH, &[]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("net-tools: netd spawn failed");
            return 1;
        }
    };
    if !wait_netd() {
        println!("net-tools: netd did not register");
        return 1;
    }
    println!("net-tools: netd ready");

    if !run(&initfs, IFCONFIG_PATH, &["ifconfig"]) {
        println!("net-tools: ifconfig list failed");
        return 1;
    }
    println!("net-tools: ifconfig list ok");
    if !run(&initfs, IFCONFIG_PATH, &["ifconfig", "lo", "10.1.2.3/24"])
        || !run(&initfs, IFCONFIG_PATH, &["ifconfig", "lo"])
        || !run(&initfs, IFCONFIG_PATH, &["ifconfig", "lo", "127.0.0.1/8"])
        || !run(&initfs, IFCONFIG_PATH, &["ifconfig", "lo", "up"])
    {
        println!("net-tools: ifconfig set failed");
        return 1;
    }
    println!("net-tools: ifconfig set ok");

    for (port, udp) in [(7000u16, false), (7001u16, true)] {
        let port_text = if port == 7000 { "7000" } else { "7001" };
        let mut listener_args = alloc::vec!["nc", "-l", "-p", port_text, "-e", "-w", "10"];
        let mut client_args = alloc::vec![
            "nc",
            "-s",
            "hello",
            "-n",
            "5",
            "-w",
            "10",
            "127.0.0.1",
            port_text,
        ];
        if udp {
            listener_args.insert(1, "-u");
            client_args.insert(1, "-u");
            client_args.insert(2, "-b");
            client_args.insert(3, "7102");
        }
        let listener = match spawn_child(&initfs, NC_PATH, &listener_args) {
            Ok(handle) => handle,
            Err(_) => {
                println!("net-tools: nc listener spawn failed");
                return 1;
            }
        };
        let _ = raven_rt::nanosleep(20_000_000);
        let client = match spawn_child(&initfs, NC_PATH, &client_args) {
            Ok(handle) => handle,
            Err(_) => {
                println!("net-tools: nc client spawn failed");
                return 1;
            }
        };
        let client_code = wait(client).unwrap_or(-1);
        let listener_code = wait(listener).unwrap_or(-1);
        if client_code != 0 || listener_code != 0 {
            println!("net-tools: nc {} failed", if udp { "udp" } else { "tcp" });
            return 1;
        }
        println!("net-tools: {} ok", if udp { "udp" } else { "tcp" });
    }

    let _ = raven_rt::proc_terminate(netd, 0);
    let _ = wait(netd);
    println!("net-tools: done");
    0
}
