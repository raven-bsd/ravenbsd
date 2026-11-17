// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S73 revoke client: the controller transfers a MemObject with MAP|READ (reduced: no DUP). This
//! client uses MAP successfully, verifies DUP is denied (EACCES), then after the controller
//! revokes MAP the next use is EPERM.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_cap_revoke::{PAGE, dup, handle_close, map_memory, nanosleep};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

/// The handle installed by the parent's `HANDLE_TRANSFER` (first free slot in the child).
const OBJ: Handle = Handle::new(3, 0);

fn map_read(handle: Handle) -> Result<usize, Errno> {
    map_memory(
        Handle::SELF_PROC,
        handle,
        0,
        0,
        PAGE,
        map::pack(prot::READ, 0),
    )
}

fn main(_args: raven_rt::Args) -> i32 {
    // Use it: MAP is granted.
    match map_read(OBJ) {
        Ok(_) => println!("cap: use ok"),
        Err(error) => {
            println!("cap: use errno={}", error.0);
            return 1;
        }
    }
    // Reduced rights: DUP was never granted.
    match dup(OBJ, Rights::DUP) {
        Err(Errno::EACCES) => println!("cap: reduced ok"),
        other => {
            println!("cap: reduced not EACCES ({other:?})");
            return 1;
        }
    }
    // The controller revokes MAP on the lineage during this sleep.
    nanosleep(800_000_000);
    match map_read(OBJ) {
        Err(Errno::EPERM) => println!("cap: revoke eperm ok"),
        other => {
            println!("cap: revoke not EPERM ({other:?})");
            return 1;
        }
    }
    handle_close(OBJ);
    0
}
