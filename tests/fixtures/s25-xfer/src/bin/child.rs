// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S25 child: its parent transfers the pattern memory object before this first thread starts.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::syscall6;
use raven_s25_xfer as _;

entry!(main);

const PAGE: usize = 4096;
const TRANSFERRED: Handle = Handle::new(3, 0);
const TRANSFERRED_INITFS: Handle = Handle::new(4, 0);
const PATTERN: u8 = 0xa5;

fn map_handle(object: Handle, protection: usize, shared: bool) -> Result<usize, Errno> {
    let flags = if shared {
        map::pack(protection, map::SHARED)
    } else {
        map::pack(protection, 0)
    };
    // SAFETY: MAP follows the stable syscall ABI; the kernel validates every argument.
    let result = unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            object.0 as usize,
            0,
            0,
            PAGE,
            flags,
        )
    };
    Errno::from_ret(result)
}

fn map_memory(protection: usize, shared: bool) -> Result<usize, Errno> {
    map_handle(TRANSFERRED, protection, shared)
}

fn main(_args: raven_rt::Args) -> i32 {
    let address = match map_memory(prot::READ, false) {
        Ok(address) => address,
        Err(_) => return 1,
    };
    // SAFETY: MAP just returned a readable, page-sized mapping at `address`.
    let first = unsafe { (address as *const u8).read() };
    if first != PATTERN {
        return 2;
    }
    let initfs = match map_handle(TRANSFERRED_INITFS, prot::READ, false) {
        Ok(address) => address,
        Err(error) => {
            println!("s25: initfs map errno={}", error.0);
            return 8;
        }
    };
    // SAFETY: MAP just returned a readable, page-sized mapping at `initfs`.
    let magic = unsafe { core::slice::from_raw_parts(initfs as *const u8, 8) };
    if magic != b"RAVENFS1" {
        println!("s25: initfs magic");
        return 9;
    }
    // The handle carries READ|MAP without WRITE: a shared writable mapping must be refused.
    // (A private writable mapping is copy-on-write and needs only MAP, by design.)
    match map_memory(prot::READ | prot::WRITE, true) {
        Err(Errno::EACCES) => 7,
        _ => 3,
    }
}
