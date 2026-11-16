// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B23 fixture controller.
//!
//! Starts `initfsd`, `ramfsd` and `vfsd`, waits until `file:/tmp` resolves, then runs `linuxd` on
//! `/hello`. The probe inside linuxd opens `/data.txt` (initfs through `file:`) and creates and
//! rewrites `/tmp/b23.txt` (ramfs through `file:`), exercising the B23 file syscall batch.

#![no_std]
#![no_main]

use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn_with};
use raven_rt::sys;

entry!(main);

const RETRIES: usize = 500;
const RETRY_NS: u64 = 10_000_000;
const O_RDONLY: u32 = 0;
const O_DIRECTORY: u32 = 0x1_0000;

fn main(_args: raven_rt::Args) -> i32 {
    let image = match sys::map_initfs() {
        Ok(image) => image,
        Err(error) => {
            println!("linux-files: FAIL initfs map: {}", error.description());
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("linux-files: FAIL initfs parse");
            return 1;
        }
    };
    let initfsd = match spawn_role(&initfs, "/sbin/initfsd", &[], &["INITFS_HANDLE=1"]) {
        Ok(handle) => handle,
        Err(error) => {
            println!("linux-files: FAIL spawn initfsd: {error:?}");
            return 1;
        }
    };
    if let Err(error) = wait_scheme("initfs:") {
        println!("linux-files: FAIL wait initfs: {}", error.description());
        return 1;
    }
    let ramfsd = match spawn_role(&initfs, "/sbin/ramfsd", &[], &[]) {
        Ok(handle) => handle,
        Err(error) => {
            println!("linux-files: FAIL spawn ramfsd: {error:?}");
            return 1;
        }
    };
    if let Err(error) = wait_scheme("ramfs:") {
        println!("linux-files: FAIL wait ramfs: {}", error.description());
        return 1;
    }
    let vfsd = match spawn_role(&initfs, "/sbin/vfsd", &[], &[]) {
        Ok(handle) => handle,
        Err(error) => {
            println!("linux-files: FAIL spawn vfsd: {error:?}");
            return 1;
        }
    };
    if let Err(error) = wait_vfs() {
        println!("linux-files: FAIL wait file: {}", error.description());
        return 1;
    }
    println!("linux-files: file namespace ready");

    let linuxd = match spawn_role(&initfs, "/sbin/linuxd", &["/sbin/linuxd", "/hello"], &[]) {
        Ok(handle) => handle,
        Err(error) => {
            println!("linux-files: FAIL spawn linuxd: {error:?}");
            return 1;
        }
    };
    let mut rt = RtSys;
    let mut status = 0i32;
    if let Err(error) = rt.proc_wait(linuxd, &mut status) {
        println!("linux-files: FAIL wait linuxd: {}", error.description());
        return 1;
    }
    for handle in [vfsd, ramfsd, initfsd] {
        let _ = sys::proc_terminate(handle, 0);
        let mut ignored = 0i32;
        let _ = rt.proc_wait(handle, &mut ignored);
    }
    println!("linux-files: linuxd exit={status}");
    if status == 0 { 0 } else { 1 }
}

fn spawn_role(
    initfs: &Initfs<'_>,
    path: &str,
    args: &[&str],
    env: &[&str],
) -> Result<raven_abi::types::Handle, raven_rt::spawn::SpawnError> {
    let options = SpawnOptions { args, env };
    spawn_with(initfs, path, &options, &mut RtSys)
}

fn wait_scheme(path: &str) -> Result<(), raven_abi::errno::Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match raven_rt::fs::open(path, O_RDONLY) {
            Ok(file) => {
                drop(file);
                return Ok(());
            }
            Err(raven_abi::errno::Errno::ENOENT) => {
                let _ = sys::nanosleep(RETRY_NS);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(raven_abi::errno::Errno::ETIMEDOUT)
}

fn wait_vfs() -> Result<(), raven_abi::errno::Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match raven_rt::fs::open("file:/tmp", O_RDONLY | O_DIRECTORY) {
            Ok(file) => {
                drop(file);
                return Ok(());
            }
            Err(raven_abi::errno::Errno::ENOENT) => {
                let _ = sys::nanosleep(RETRY_NS);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(raven_abi::errno::Errno::ETIMEDOUT)
}
