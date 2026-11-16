// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P12 `plat-client`: query the `plat:` scheme after `devd` launches `/sbin/platd`.

#![no_std]
#![no_main]
#![forbid(unsafe_code)]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_plat::gpio::{Function, Pull};
use raven_plat::protocol::{GPIO_SET_FUNCTION, GPIO_SET_PULL};
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Client;

entry!(main);

const RETRIES: usize = 200_000;

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
    if open_retry("plat:model", 1, false).is_err() {
        println!("pi4-platd: platd missing");
        return 1;
    }

    let model = read_u32("plat:model").unwrap_or(0);
    let revision = read_u32("plat:revision").unwrap_or(0);
    let serial = read_u64("plat:serial").unwrap_or(0);
    let mut mac = [0u8; 6];
    let _ = read_path("plat:mac", &mut mac);
    let clock = read_u32("plat:clock/12").unwrap_or(0);
    let power = read_u32("plat:power/0").unwrap_or(0);
    println!("pi4-platd: model=0x{model:08x}");
    println!("pi4-platd: revision=0x{revision:08x}");
    println!("pi4-platd: serial=0x{serial:016x}");
    println!(
        "pi4-platd: mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    println!("pi4-platd: clock12={clock}");
    println!("pi4-platd: power0={power}");

    let client = Client::new();
    let claim = match client.open("plat:gpio/17", 1) {
        Ok(handle) => handle,
        Err(_) => {
            println!("pi4-platd: gpio claim failed");
            return 1;
        }
    };
    println!("pi4-platd: gpio claim ok");
    match Client::new().open("plat:gpio/17", 1) {
        Err(Errno::EBUSY) => println!("pi4-platd: gpio conflict ok"),
        _ => {
            println!("pi4-platd: gpio conflict failed");
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
    println!("pi4-platd: gpio config ok");
    let _ = client.close(claim);

    let rng_handle = match client.open("plat:rng", 1) {
        Ok(handle) => handle,
        Err(_) => {
            println!("pi4-platd: rng open failed");
            return 1;
        }
    };
    let mut rng = [0u8; 8];
    let read = client.read(rng_handle, 0, &mut rng).unwrap_or_default();
    let _ = client.close(rng_handle);
    if read != rng.len() {
        println!("pi4-platd: rng read failed");
        return 1;
    }
    raven_rt::print!("pi4-platd: rng=");
    for byte in rng {
        raven_rt::print!("{byte:02x}");
    }
    println!();
    println!("pi4-platd: done");
    0
}
