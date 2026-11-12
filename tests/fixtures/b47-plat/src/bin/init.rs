// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B47b `/init`: exercise `platd` board, GPIO, RNG and client-death reclamation.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_plat::gpio::{Function, Pull};
use raven_plat::protocol::{GPIO_SET_FUNCTION, GPIO_SET_PULL};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

const PLATD_PATH: &str = "/platd";
const CLIENT_PATH: &str = "/client";
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

fn open_retry(path: &str, flags: u32, retry_busy: bool) -> Result<Handle, Errno> {
    let client = Client::new();
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match client.open(path, flags) {
            Ok(handle) => return Ok(handle),
            Err(error)
                if error == Errno::ENOENT
                    || error == Errno::EIO
                    || (retry_busy && error == Errno::EBUSY) =>
            {
                core::hint::spin_loop();
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn read_path(path: &str, out: &mut [u8]) -> Result<(), Errno> {
    let mut file = raven_rt::fs::open(path, 0)?;
    let read = file.read(out)?;
    let _ = file.close();
    if read == out.len() {
        Ok(())
    } else {
        Err(Errno::EIO)
    }
}

fn read_u32(path: &str) -> Result<u32, Errno> {
    let mut bytes = [0u8; 4];
    read_path(path, &mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(path: &str) -> Result<u64, Errno> {
    let mut bytes = [0u8; 8];
    read_path(path, &mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn main(_args: raven_rt::Args) -> i32 {
    let Some(initfs) = map_initfs() else {
        println!("b47-plat: initfs map failed");
        return 1;
    };
    let platd = match spawn_child(&initfs, PLATD_PATH, &["fake"]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47-plat: platd spawn failed");
            return 1;
        }
    };
    if open_retry("plat:model", 1, false).is_err() {
        println!("b47-plat: platd did not register");
        return 1;
    }

    let model = read_u32("plat:model").unwrap_or(0);
    let revision = read_u32("plat:revision").unwrap_or(0);
    let serial = read_u64("plat:serial").unwrap_or(0);
    let mut mac = [0u8; 6];
    let _ = read_path("plat:mac", &mut mac);
    let clock = read_u32("plat:clock/12").unwrap_or(0);
    let power = read_u32("plat:power/0").unwrap_or(0);
    println!("b47-plat: model=0x{model:08x}");
    println!("b47-plat: revision=0x{revision:08x}");
    println!("b47-plat: serial=0x{serial:016x}");
    println!(
        "b47-plat: mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    println!("b47-plat: clock12={clock}");
    println!("b47-plat: power0={power}");

    let client = Client::new();
    let claim = match client.open("plat:gpio/17", 1) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47-plat: gpio claim failed");
            return 1;
        }
    };
    println!("b47-plat: gpio claim ok");
    match Client::new().open("plat:gpio/17", 1) {
        Err(Errno::EBUSY) => println!("b47-plat: gpio conflict ok"),
        _ => {
            println!("b47-plat: gpio conflict failed");
            return 1;
        }
    }
    let mut response = [0u8; 1];
    let _ = client.call(
        claim,
        &[GPIO_SET_FUNCTION, Function::Output.bits() as u8],
        &mut response,
    );
    let _ = client.call(
        claim,
        &[GPIO_SET_PULL, Pull::Up.bits() as u8],
        &mut response,
    );
    println!("b47-plat: gpio config ok");
    let _ = client.close(claim);
    let again = match open_retry("plat:gpio/17", 1, true) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47-plat: gpio release failed");
            return 1;
        }
    };
    let _ = client.close(again);
    println!("b47-plat: gpio release ok");

    let rng_handle = match client.open("plat:rng", 1) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47-plat: rng open failed");
            return 1;
        }
    };
    let mut rng = [0u8; 8];
    let read = client.read(rng_handle, 0, &mut rng).unwrap_or_default();
    let _ = client.close(rng_handle);
    if read != rng.len() {
        println!("b47-plat: rng read failed");
        return 1;
    }
    raven_rt::print!("b47-plat: rng=");
    for byte in rng {
        raven_rt::print!("{byte:02x}");
    }
    println!();

    let death = match spawn_child(&initfs, CLIENT_PATH, &[]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47-plat: client spawn failed");
            return 1;
        }
    };
    let _ = wait(death);
    let reclaimed = match open_retry("plat:gpio/27", 1, true) {
        Ok(handle) => handle,
        Err(_) => {
            println!("b47-plat: reclaim failed");
            return 1;
        }
    };
    let _ = client.close(reclaimed);
    println!("b47-plat: reclaim ok");

    let _ = raven_rt::proc_terminate(platd, 0);
    let _ = wait(platd);
    println!("b47-plat: done");
    0
}
