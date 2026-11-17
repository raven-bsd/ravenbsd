// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S73 lifetime client: the controller transfers a MemObject with MAP, then releases its own
//! source handle while this client still holds the derived handle. The object must stay alive
//! (no use-after-free), be released when this client closes its handle, and the stale handle must
//! then fail with EBADF rather than touch the reused slot.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, map, prot};
use raven_cap_revoke::{PAGE, handle_close, map_memory, memobj_create, nanosleep};
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
    match map_read(OBJ) {
        Ok(_) => println!("cap: lifetime use ok"),
        Err(error) => {
            println!("cap: lifetime use errno={}", error.0);
            return 1;
        }
    }
    // The controller releases its source handle during this sleep; the object is then held only
    // by this client, so it must not be freed.
    nanosleep(800_000_000);
    match map_read(OBJ) {
        Ok(_) => println!("cap: lifetime alive ok"),
        Err(error) => {
            println!("cap: lifetime alive errno={}", error.0);
            return 1;
        }
    }
    // Release the last live handle; the object is now freed.
    handle_close(OBJ);
    // A new object reuses the slot with a fresh generation.
    let fresh = match memobj_create(PAGE) {
        Ok(handle) => handle,
        Err(error) => {
            println!("cap: lifetime fresh errno={}", error.0);
            return 1;
        }
    };
    match map_read(fresh) {
        Ok(_) => println!("cap: lifetime reuse ok"),
        Err(error) => {
            println!("cap: lifetime reuse errno={}", error.0);
            return 1;
        }
    }
    // The stale generation-0 handle must fail with EBADF, not touch the reused slot.
    match map_read(OBJ) {
        Err(Errno::EBADF) => println!("cap: lifetime stale ok"),
        other => {
            println!("cap: lifetime stale not EBADF ({other:?})");
            return 1;
        }
    }
    handle_close(fresh);
    0
}
