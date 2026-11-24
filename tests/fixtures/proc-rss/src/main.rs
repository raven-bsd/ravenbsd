// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-process RSS fixture: map a known number of pages, then assert that
//! `PROC_GETRUSAGE` reports a nonzero `maxrss` that tracks the mapping.

#![no_std]
#![no_main]

extern crate alloc;

#[path = "../../proc-times/src/common.rs"]
mod common;

use raven_rt::entry;
use raven_rt::println;

use common::{PAGE, getrusage};

entry!(main);

/// Pages mapped for the RSS assertion (256 KiB).
const RSS_PAGES: usize = 32;

fn main(_args: raven_rt::Args) -> i32 {
    let before = match getrusage() {
        Ok(value) => value[2],
        Err(_) => {
            println!("proc-rss: fail before");
            return 1;
        }
    };
    let mut block: alloc::vec::Vec<u8> = alloc::vec![0u8; RSS_PAGES * PAGE];
    for page in 0..RSS_PAGES {
        block[page * PAGE] = (page as u8).wrapping_add(1);
    }
    let after = match getrusage() {
        Ok(value) => value[2],
        Err(_) => {
            println!("proc-rss: fail after");
            return 1;
        }
    };
    let delta = after.saturating_sub(before);
    if after == 0 || delta < (RSS_PAGES * PAGE) as u64 {
        println!("proc-rss: fail rss before={before} after={after} delta={delta}");
        return 1;
    }
    println!("proc-rss: rss=1 bytes={after} delta={delta}");
    core::hint::black_box(&block);
    println!("proc-rss: ok");
    0
}
