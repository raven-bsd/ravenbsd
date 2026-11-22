// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S73 revoke client: the controller installs two capabilities on the same MemObject, handle 3
//! with MAP|READ|DUP and handle 4 with READ|DUP (never MAP). The client maps through 3, duplicates
//! 3 before the revoke, then sleeps while the controller revokes MAP. After the revoke every use
//! of MAP on the lineage must fail `EPERM` (a bit that was granted and revoked), while a handle
//! that never had MAP stays `EACCES` and a fresh duplicate can only keep `READ` (`EACCES` for
//! MAP). The controller starts this binary and a `PROC_FORK` clone, so both processes must observe
//! the same rejection.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_cap_revoke::{PAGE, dup, handle_close, map_memory, nanosleep};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

/// MAP|READ|DUP capability installed by the parent (first free slot).
const OBJ: Handle = Handle::new(3, 0);
/// READ|DUP capability on the same object; it never had MAP.
const NEVER: Handle = Handle::new(4, 0);

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

fn fail(what: &str, error: Errno) -> i32 {
    println!("cap: {what} errno={}", error.0);
    1
}

fn main(_args: raven_rt::Args) -> i32 {
    // Use it: MAP is granted on handle 3.
    if let Err(error) = map_read(OBJ) {
        return fail("use", error);
    }
    println!("cap: use ok");

    // A pre-revoke duplicate that also holds MAP is a descendant; the later revoke must reach it.
    let dup_before = match dup(OBJ, Rights::MAP.union(Rights::READ).union(Rights::DUP)) {
        Ok(handle) => handle,
        Err(error) => return fail("dup-before", error),
    };
    println!("cap: dup-before ok");

    // Handle 4 never had MAP: the error is EACCES, not EPERM.
    match map_read(NEVER) {
        Err(Errno::EACCES) => println!("cap: never-granted ok"),
        Ok(_) => {
            println!("cap: never-granted unexpected ok");
            return 1;
        }
        Err(error) => return fail("never-granted", error),
    }
    let never_dup = match dup(NEVER, Rights::READ) {
        Ok(handle) => handle,
        Err(error) => return fail("never-dup", error),
    };

    // The controller revokes MAP on the shared lineage during this sleep.
    nanosleep(800_000_000);

    match map_read(OBJ) {
        Err(Errno::EPERM) => println!("cap: revoke eperm ok"),
        Ok(_) => {
            println!("cap: revoke unexpected ok");
            return 1;
        }
        Err(error) => return fail("revoke", error),
    }
    match map_read(dup_before) {
        Err(Errno::EPERM) => println!("cap: dup-before eperm ok"),
        Ok(_) => {
            println!("cap: dup-before unexpected ok");
            return 1;
        }
        Err(error) => return fail("dup-before", error),
    }

    // A duplicate created after the revoke cannot regain MAP; the new handle has READ only, so
    // MAP is a never-granted bit on that node (EACCES), not a revoked bit.
    let dup_after = match dup(OBJ, Rights::READ) {
        Ok(handle) => handle,
        Err(error) => return fail("dup-after", error),
    };
    println!("cap: dup-after ok");
    match map_read(dup_after) {
        Err(Errno::EACCES) => println!("cap: dup-after eacces ok"),
        Ok(_) => {
            println!("cap: dup-after unexpected ok");
            return 1;
        }
        Err(error) => return fail("dup-after", error),
    }
    match map_read(NEVER) {
        Err(Errno::EACCES) => println!("cap: never-after eacces ok"),
        Ok(_) => {
            println!("cap: never-after unexpected ok");
            return 1;
        }
        Err(error) => return fail("never-after", error),
    }
    match map_read(never_dup) {
        Err(Errno::EACCES) => println!("cap: never-dup eacces ok"),
        Ok(_) => {
            println!("cap: never-dup unexpected ok");
            return 1;
        }
        Err(error) => return fail("never-dup", error),
    }

    handle_close(dup_before);
    handle_close(dup_after);
    handle_close(never_dup);
    handle_close(OBJ);
    handle_close(NEVER);
    println!("cap: revoke client ok");
    0
}
