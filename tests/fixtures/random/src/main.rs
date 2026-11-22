// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K524: AT_RANDOM and GETRANDOM share the boot-seeded pool, never xorshift.
//!
//! The seeded mode starts the file services, records its AT_RANDOM, spawns `/child` (a fresh
//! exec), compares the two 16-byte values, and draws two GETRANDOM buffers. The `entropy=none`
//! mode has no AT_RANDOM and expects GETRANDOM to return `EAGAIN`.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};

use raven_abi::errno::Errno;
use raven_initfs::Initfs;
use raven_proto_fs::{O_CREAT, O_RDONLY, O_WRONLY};
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn};

entry!(main);

const AT_RANDOM: usize = 25;
const GETRANDOM: usize = 0x08;
const DIR: &str = "/tmp/random";
const CHILD_FILE: &str = "/tmp/random/child";

fn main(args: raven_rt::Args) -> i32 {
    if args.get(1) == Some("child") {
        return child(&args);
    }
    match read_random(&args) {
        Some(random) => seeded(&args, random),
        None => unseeded(),
    }
}

fn read_random(args: &raven_rt::Args) -> Option<[u8; 16]> {
    let pointer = args.auxv(AT_RANDOM)?;
    if pointer == 0 {
        return None;
    }
    let mut random = [0u8; 16];
    // SAFETY: the kernel's AT_RANDOM auxv value names a 16-byte readable stack slot.
    unsafe {
        core::ptr::copy_nonoverlapping(pointer as *const u8, random.as_mut_ptr(), random.len());
    }
    Some(random)
}

fn child(args: &raven_rt::Args) -> i32 {
    let Some(random) = read_random(args) else {
        println!("random: child missing at-random");
        return 1;
    };
    if write_file(CHILD_FILE, &random).is_err() {
        println!("random: child write failed");
        return 1;
    }
    println!("random: child ok");
    0
}

fn seeded(args: &raven_rt::Args, parent_random: [u8; 16]) -> i32 {
    if let Err(error) = start_services() {
        println!("random: fail {error}");
        return 1;
    }
    if let Err(error) = setup() {
        println!("random: fail {error}");
        return 1;
    }
    let image = match raven_rt::map_initfs() {
        Ok(image) => image,
        Err(_) => {
            println!("random: fail initfs");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("random: fail initfs parse");
            return 1;
        }
    };
    let child = match spawn(
        &initfs,
        "/child",
        &SpawnOptions {
            args: &["child", "child"],
            env: &[],
        },
    ) {
        Ok(child) => child,
        Err(_) => {
            println!("random: fail spawn");
            return 1;
        }
    };
    let mut code = -1i32;
    if RtSys.proc_wait(child, &mut code).is_err() || code != 0 {
        println!("random: fail child code={code}");
        return 1;
    }
    let child_random = match read_file(CHILD_FILE) {
        Ok(bytes) if bytes.len() == 16 => {
            let mut value = [0u8; 16];
            value.copy_from_slice(&bytes);
            value
        }
        _ => {
            println!("random: fail child file");
            return 1;
        }
    };
    if parent_random == [0u8; 16] {
        println!("random: fail zero parent AT_RANDOM");
        return 1;
    }
    if child_random == [0u8; 16] {
        println!("random: fail zero child AT_RANDOM");
        return 1;
    }
    if child_random == parent_random {
        println!("random: exec differ=0 (parent stream repeated)");
        return 1;
    }
    let mut first = [0u8; 16];
    let mut second = [0u8; 16];
    if raw_getrandom(&mut first).is_err() || raw_getrandom(&mut second).is_err() {
        println!("random: fail getrandom");
        return 1;
    }
    if first == second || first == [0u8; 16] || first == parent_random {
        println!("random: fail getrandom stream");
        return 1;
    }
    let _ = args;
    println!("random: exec differ=1");
    println!("random: getrandom ok=1");
    println!("random: ok");
    0
}

fn unseeded() -> i32 {
    let mut first = [0u8; 16];
    match raw_getrandom(&mut first) {
        Err(Errno::EAGAIN) => {}
        other => {
            println!("random: fail unseeded getrandom {other:?}");
            return 1;
        }
    }
    println!("random: at-random absent");
    println!("random: getrandom eagain");
    println!("random: ok");
    0
}

fn raw_getrandom(buffer: &mut [u8]) -> Result<usize, Errno> {
    // SAFETY: GETRANDOM writes at most `buffer.len()` bytes to the caller's live slice.
    let result = unsafe {
        raven_rt::sys::syscall3(GETRANDOM, buffer.as_mut_ptr() as usize, buffer.len(), 0)
    };
    Errno::from_ret(result)
}

fn start_services() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map".to_string())?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse".to_string())?;
    for path in ["/sbin/initfsd", "/sbin/ramfsd", "/sbin/vfsd"] {
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| format!("spawn {path}"))?;
        let scheme = match path {
            "/sbin/initfsd" => "initfs:",
            "/sbin/ramfsd" => "ramfs:",
            _ => "file:",
        };
        wait_scheme(scheme)?;
    }
    Ok(())
}

fn wait_scheme(scheme: &str) -> Result<(), String> {
    for _ in 0..3000 {
        if fs::open(scheme, O_RDONLY).is_ok() {
            return Ok(());
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
    Err(format!("scheme {scheme} timeout"))
}

fn setup() -> Result<(), String> {
    match fs::mkdir(DIR, 0o755) {
        Ok(()) | Err(Errno::EEXIST) => Ok(()),
        Err(error) => Err(format!("mkdir {DIR}: {}", error.description())),
    }
}

fn write_file(path: &str, data: &[u8]) -> Result<(), Errno> {
    let mut file = fs::open(&format!("file:{path}"), O_WRONLY | O_CREAT)?;
    let written = file.write(data)?;
    file.close()?;
    if written != data.len() {
        return Err(Errno::EIO);
    }
    Ok(())
}

fn read_file(path: &str) -> Result<alloc::vec::Vec<u8>, Errno> {
    fs::read_to_vec(&format!("file:{path}"))
}
