// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B71a spawn-smoke: runtime `spawn`, `PROC_WAIT` and `PROC_SELF_INFO`.
//!
//! `/init` spawns the real `/child` from the kernel-mapped initfs through
//! `raven_rt::spawn`, waits for exit code 7 and reads its own pid. This isolates
//! the runtime side of the B71 boundary from the libc `posix_spawn` wrapper.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn_with};
use raven_rt::sys::{map_initfs, syscall2};

entry!(main);

const PROC_SELF_INFO: usize = 0x35;

fn main(_args: raven_rt::Args) -> i32 {
    println!("spawn-smoke: start");
    let pid = match self_pid() {
        Ok(pid) if pid > 0 => pid,
        Ok(_) => {
            println!("spawn-smoke: fail getpid");
            return 1;
        }
        Err(error) => {
            println!("spawn-smoke: fail getpid {}", error.description());
            return 1;
        }
    };

    let image = match map_initfs() {
        Ok(image) => image,
        Err(error) => {
            println!("spawn-smoke: fail initfs {}", error.description());
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("spawn-smoke: fail initfs");
            return 1;
        }
    };
    let mut sys = RtSys;
    let options = SpawnOptions {
        args: &["/child"],
        env: &[],
    };
    let child = match spawn_with(&initfs, "/child", &options, &mut sys) {
        Ok(child) => child,
        Err(error) => {
            println!("spawn-smoke: fail spawn {:?}", error);
            return 1;
        }
    };
    let mut code = -1;
    if let Err(error) = sys.proc_wait(child, &mut code) {
        println!("spawn-smoke: fail wait {}", error.description());
        return 1;
    }
    println!("spawn-smoke: ok pid={pid} exit={code}");
    if code == 7 { 0 } else { 1 }
}

fn self_pid() -> Result<i32, Errno> {
    let mut words = [0u64; 8];
    // SAFETY: PROC_SELF_INFO writes at most `words.len()` u64 words into the array.
    let raw = unsafe { syscall2(PROC_SELF_INFO, words.as_mut_ptr() as usize, words.len()) };
    Errno::from_ret(raw)?;
    i32::try_from(words[0]).map_err(|_| Errno::EINVAL)
}
