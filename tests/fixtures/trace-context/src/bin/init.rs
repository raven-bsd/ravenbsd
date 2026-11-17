// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Q3 trace-context controller: prove the `raven-proto-trace` codec round-trips on the target and
//! that the scheme call/reply path is green. The trace-id stamping on the kernel header is the
//! deferred kernel-integration part (S69 owns `kernel/src/ipc/**`).

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_proto_trace::{UNTRACED, format, is_untraced, parse};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};

entry!(main);

/// Open flags: read only.
const O_READ: u32 = 1;
/// Open retry budget for the server registration.
const OPEN_RETRIES: usize = 200_000;

fn fail(message: &str) -> i32 {
    println!("trace: failed {message}");
    1
}

fn codec_round_trip() -> Result<(), &'static str> {
    // A value that exercises all 8 bytes.
    let id = 0x0123_4567_89AB_CDEFu64;
    if parse(&format(id)) != Ok(id) {
        return Err("codec round-trip");
    }
    if parse(&format(0)) != Ok(0) || !is_untraced(0) {
        return Err("codec missing");
    }
    // A malformed (too-short) field is a typed error, never a fault.
    if !parse(&[0u8; 4]).is_err() || is_untraced(UNTRACED) == false {
        return Err("codec malformed");
    }
    Ok(())
}

fn wait(process: Handle) -> Result<i32, Errno> {
    let mut code = -1i32;
    // SAFETY: PROC_WAIT writes one i32 into this live stack slot.
    let raw = unsafe {
        raven_rt::sys::syscall2(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            &mut code as *mut i32 as usize,
        )
    };
    Errno::from_ret(raw).map(|_| code)
}

fn scheme_round_trip(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let server = spawn(
        initfs,
        "/server",
        &SpawnOptions {
            args: &[],
            env: &[],
        },
    )
    .map_err(|_| "server spawn")?;

    let mut attempt = 0usize;
    let file = loop {
        match raven_rt::sys::open("trace:probe", O_READ) {
            Ok(file) => break file,
            Err(Errno::ENOENT) | Err(Errno::EIO) if attempt < OPEN_RETRIES => {
                core::hint::spin_loop();
                attempt += 1;
            }
            Err(_) => return Err("open"),
        }
    };

    let mut byte = [0u8; 1];
    match raven_rt::sys::read(file, &mut byte) {
        Ok(1) if byte[0] == b'R' => {}
        _ => return Err("read"),
    }
    let _ = raven_rt::sys::close(file);

    let _ = raven_rt::proc_terminate(server, 0);
    match wait(server) {
        Ok(0) => {}
        _ => return Err("server wait"),
    }
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(message) = codec_round_trip() {
        return fail(message);
    }
    println!("trace: codec ok");

    let image = match raven_rt::map_initfs() {
        Ok(image) => image,
        Err(_) => return fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => return fail("initfs parse"),
    };
    if let Err(message) = scheme_round_trip(&initfs) {
        return fail(message);
    }
    println!("trace: scheme ok");
    println!("trace: ok");
    0
}
