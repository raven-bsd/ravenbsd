// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S25 child: its parent transfers the pattern memory object before this first thread starts.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::sys::syscall6;
use raven_s25_xfer as _;

entry!(main);

const PAGE: usize = 4096;
const TRANSFERRED: Handle = Handle::new(3, 0);
const PATTERN: u8 = 0xa5;

fn map_memory(protection: usize) -> Result<usize, Errno> {
    // SAFETY: MAP follows the stable syscall ABI; the kernel validates every argument.
    let result = unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            TRANSFERRED.0 as usize,
            0,
            0,
            PAGE,
            map::pack(protection, 0),
        )
    };
    Errno::from_ret(result)
}

fn main(_args: raven_rt::Args) -> i32 {
    let address = match map_memory(prot::READ) {
        Ok(address) => address,
        Err(_) => return 1,
    };
    // SAFETY: MAP just returned a readable, page-sized mapping at `address`.
    let first = unsafe { (address as *const u8).read() };
    if first != PATTERN {
        return 2;
    }
    match map_memory(prot::READ | prot::WRITE) {
        Err(Errno::EACCES) => 7,
        _ => 3,
    }
}
