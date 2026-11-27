// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S209 x86_64 TLB shootdown ack-order fixture.
//!
//! Map and unmap anonymous memory on every round; each unmap publishes a remote TLB shootdown.
//! The `s209-tlb-ack-order` boot flag arms the one-shot ordering injection on the first such
//! shootdown; this fixture only supplies the shootdowns and reports completion.

#![no_std]
#![no_main]

use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const LEN: usize = 1 << 20;

fn main(_args: raven_rt::Args) -> i32 {
    let mut round = 0usize;
    while round < 8 {
        let address = match sys::map_anonymous(LEN) {
            Ok(address) => address,
            Err(_) => {
                println!("s209-shootdown: failed map");
                return 1;
            }
        };
        // Touch the mapping so the unmap changes a present PTE.
        // SAFETY: `map_anonymous` returned a writable mapping of `LEN` bytes.
        unsafe { core::ptr::write_volatile(address as *mut u8, 1) };
        if sys::unmap(Handle::SELF_PROC, address, LEN).is_err() {
            println!("s209-shootdown: failed unmap");
            return 1;
        }
        round = round.saturating_add(1);
    }
    println!("s209-shootdown: ok rounds={round}");
    0
}
