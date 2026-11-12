// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S17 fault client: maps the pager object from the path argument and touches one absent page.
//!
//! The error object answers with a short supply and the death object makes the server exit, so
//! both runs must end in a pager fault instead of returning to this code.

#![no_std]
#![no_main]

use raven_abi::types::prot;
use raven_rt::entry;
use raven_rt::println;
use raven_s17_pager::{O_RDWR, PAGE, exit, fail, map_object, open};

entry!(main);

fn main(args: raven_rt::Args) -> i32 {
    let path = match args.get(1) {
        Some(path) => path,
        None => fail("fault-client path"),
    };
    let object = match open(path, O_RDWR) {
        Ok(object) => object,
        Err(_) => fail("fault-client open"),
    };
    let address = match map_object(object, 0, PAGE, prot::READ | prot::WRITE, true) {
        Ok(address) => address,
        Err(_) => fail("fault-client map"),
    };
    println!("s17: fault-client fault {path}");
    // SAFETY: `address` is a one-page mapping of the pager object. The read is volatile so the
    // compiler cannot fold away the faulting instruction.
    let value = unsafe { core::ptr::read_volatile(address as *const u8) };
    println!("s17: fault-client unexpected value={value}");
    exit(1)
}
