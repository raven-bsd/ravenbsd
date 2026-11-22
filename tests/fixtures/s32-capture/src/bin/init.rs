// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S32 controller: cooperative server, canceller, then the two-thread capture client.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_rt::sys::proc_terminate;
use raven_s32_capture::{
    O_RDWR, call, close, map_initfs_image, nanosleep, open, proc_try_wait, proc_wait,
};

entry!(main);

const SERVER: &str = "/server";
const CLIENT: &str = "/client";
const CANCELLER: &str = "/canceller";
const RETRIES: usize = 2000;

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = raven_s32_capture::nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn spawn_child(initfs: &Initfs<'_>, path: &str) -> Result<Handle, Errno> {
    spawn_child_args(initfs, path, &[])
}

fn spawn_child_args(initfs: &Initfs<'_>, path: &str, args: &[&str]) -> Result<Handle, Errno> {
    let options = SpawnOptions { args, env: &[] };
    spawn(initfs, path, &options).map_err(|_| Errno::EIO)
}

/// K511 matrix: 10k capture cycles, the 8-slot burst bound, a cancelled capture whose server
/// still replies, and a revoked file handle with a capture in flight.
fn matrix(initfs: &Initfs<'_>) -> Result<(), Errno> {
    let cycle = spawn_child_args(initfs, CLIENT, &["client", "cycle"])?;
    if !wait_ok(cycle) {
        println!("s32: cycle failed");
        return Err(Errno::EIO);
    }

    // Each 1 MiB capture is 256 pages; one 4 MiB slot holds four. Eight slots -> 32 captures.
    let mut burst = [Handle::NONE; 40];
    let mut success = 0u32;
    let mut full = 0u32;
    for slot in &mut burst {
        *slot = spawn_child_args(initfs, CLIENT, &["client", "burst"])?;
    }
    // Let all forty clients reach the capture allocator; the server holds the first 32.
    let _ = nanosleep(500_000_000);
    // OPEN has no capture, so it reaches the server while all 32 held slots are full.
    let release = open("s32:release", O_RDWR)?;
    let _ = close(release);
    for child in burst {
        match proc_wait(child) {
            Ok(0) => success = success.saturating_add(1),
            Ok(3) => full = full.saturating_add(1),
            _ => {
                println!("s32: burst child failed");
                return Err(Errno::EIO);
            }
        }
    }
    if success != 32 || full != 8 {
        println!("s32: bound failed success={success} full={full}");
        return Err(Errno::EIO);
    }
    println!("s32: bound ok success={success} full={full}");

    let victim = spawn_child_args(initfs, CLIENT, &["client", "victim"])?;
    let _ = nanosleep(30_000_000);
    proc_terminate(victim, 137)?;
    let _ = proc_wait(victim);
    // Let the server finish its 150 ms fill and the late reply release the cancelled capture.
    let _ = nanosleep(300_000_000);
    println!("s32: cancel-capture ok");

    let revoke = spawn_child_args(initfs, CLIENT, &["client", "revoke"])?;
    if !wait_ok(revoke) {
        println!("s32: revoke failed");
        return Err(Errno::EIO);
    }

    let close_request = spawn_child_args(initfs, CLIENT, &["client", "close-request"])?;
    let mut status = None;
    for _ in 0..100 {
        if let Ok(Some(code)) = proc_try_wait(close_request) {
            status = Some(code);
            break;
        }
        let _ = nanosleep(10_000_000);
    }
    if status != Some(0) {
        let _ = proc_terminate(close_request, 137);
        println!("s32: close-request failed status={status:?}");
        return Err(Errno::EIO);
    }
    println!("s32: close-request ok");

    let revoke_request = spawn_child_args(initfs, CLIENT, &["client", "revoke-request"])?;
    let mut status = None;
    for _ in 0..100 {
        if let Ok(Some(code)) = proc_try_wait(revoke_request) {
            status = Some(code);
            break;
        }
        let _ = nanosleep(10_000_000);
    }
    if status != Some(0) {
        let _ = proc_terminate(revoke_request, 137);
        println!("s32: revoke-request failed status={status:?}");
        return Err(Errno::EIO);
    }
    println!("s32: revoke-request ok");
    Ok(())
}

fn wait_ok(child: Handle) -> bool {
    matches!(proc_wait(child), Ok(0))
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => {
            println!("s32: initfs map failed");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("s32: initfs parse failed");
            return 1;
        }
    };
    let server = match spawn_child(&initfs, SERVER) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s32: server spawn failed");
            return 1;
        }
    };
    let control = match open_retry("s32:echo") {
        Ok(handle) => handle,
        Err(_) => {
            println!("s32: server did not register");
            return 1;
        }
    };

    let canceller = match spawn_child(&initfs, CANCELLER) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s32: canceller spawn failed");
            return 1;
        }
    };
    if !wait_ok(canceller) {
        println!("s32: canceller child failed");
        return 1;
    }
    if call(control, b"alive", None).is_err() {
        println!("s32: server did not survive the cancellation");
        return 1;
    }
    println!("s32: cancel ok");

    if matrix(&initfs).is_err() {
        println!("s32: matrix failed");
        return 1;
    }

    let client = match spawn_child(&initfs, CLIENT) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s32: client spawn failed");
            return 1;
        }
    };
    if !wait_ok(client) {
        println!("s32: capture client failed");
        return 1;
    }
    let _ = proc_wait(server);
    let _ = close(control);
    println!("s32: lifecycle ok");
    0
}
