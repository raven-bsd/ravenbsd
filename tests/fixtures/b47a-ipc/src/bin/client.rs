// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B47a fixture client: one normal round trip or one deferred client-death request.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Client;

entry!(main);

const O_RDONLY: u32 = 1;
const O_RDWR: u32 = 3;

fn roundtrip() -> Result<(), Errno> {
    let api = Client::new();
    let write_file = api.open("b47a:file", O_RDWR)?;
    if api.write(write_file, 0, b"b47a")? != 4 {
        return Err(Errno::EIO);
    }
    api.close(write_file)?;

    let read_file = api.open("b47a:file", O_RDONLY)?;
    let mut buffer = [0u8; 8];
    let length = api.read(read_file, 0, &mut buffer)?;
    api.close(read_file)?;
    if length != 4 || &buffer[..4] != b"b47a" {
        return Err(Errno::EIO);
    }

    let call_file = api.open("b47a:file", O_RDONLY)?;
    let mut echo = [0u8; 8];
    let length = api.call(call_file, b"ping", &mut echo)?;
    api.close(call_file)?;
    if length != 4 || &echo[..4] != b"ping" {
        return Err(Errno::EIO);
    }
    Ok(())
}

fn death() -> Result<(), Errno> {
    let api = Client::new();
    let file = api.open("b47a:hold", O_RDONLY)?;
    let mut response = [0u8; 4];
    let _ = api.call(file, b"hold", &mut response)?;
    Err(Errno::EIO)
}

fn main(args: raven_rt::Args) -> i32 {
    let result = if args.get(0) == Some("death") {
        death()
    } else {
        roundtrip()
    };
    match result {
        Ok(()) => {
            println!("b47a: client ok");
            0
        }
        Err(error) => {
            println!("b47a: client failed: {}", error.description());
            1
        }
    }
}
