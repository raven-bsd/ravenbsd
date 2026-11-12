// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S16 revocation client: its parent transfers a MemObject and revokes MAP before this thread
//! starts. MAP must fail EPERM; a later DUP of the remaining rights must succeed.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_s16_rights::{PAGE, dup, map_memory};

entry!(main);

/// First handle installed by the parent's `HANDLE_TRANSFER`.
const TRANSFERRED: Handle = Handle::new(3, 0);

fn main(_args: raven_rt::Args) -> i32 {
    match map_memory(
        Handle::SELF_PROC,
        TRANSFERRED,
        0,
        0,
        PAGE,
        map::pack(prot::READ, 0),
    ) {
        Err(Errno::EPERM) => {}
        Ok(_) => {
            println!("s16: cross map accepted");
            return 1;
        }
        Err(error) => {
            println!("s16: cross map errno={}", error.0);
            return 1;
        }
    }
    let duplicate = match dup(TRANSFERRED, Rights::DUP.union(Rights::READ)) {
        Ok(handle) => handle,
        Err(error) => {
            println!("s16: cross dup errno={}", error.0);
            return 1;
        }
    };
    match map_memory(
        Handle::SELF_PROC,
        duplicate,
        0,
        0,
        PAGE,
        map::pack(prot::READ, 0),
    ) {
        Err(Errno::EACCES) => {}
        _ => {
            println!("s16: cross leaf map not EACCES");
            return 1;
        }
    }
    println!("s16: revoke client ok");
    0
}
