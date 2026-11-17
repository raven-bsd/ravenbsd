// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S72 `/init` fixture: wall clock and entropy.
//!
//! 1. `CLOCK_GET(REALTIME)` twice: the epoch must be plausible (2001..2100) and non-decreasing.
//! 2. `CLOCK_SET` to a fixed epoch, then `CLOCK_GET` must not report earlier than it.
//! 3. `GETRANDOM` twice: full-length, not all-zero, and the two draws differ.
//!
//! Any regression prints `time-entropy: fail ...` and exits nonzero; the qtest case treats that
//! as a failure.

#![no_std]
#![no_main]

use raven_abi::types::{Timespec, clock};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::{clock_get, syscall2, syscall3};

entry!(main);

/// `getrandom` syscall number (reviewer-assigned, S72; mirrors `basic::GETRANDOM`).
const GETRANDOM: usize = 0x08;
/// `clock_set` syscall number (reviewer-assigned, S72; mirrors `basic::CLOCK_SET`).
const CLOCK_SET: usize = 0x09;

/// 2001-01-01.
const MIN_EPOCH: i64 = 978_307_200;
/// 2100-01-01.
const MAX_EPOCH: i64 = 4_102_444_800;
/// A fixed epoch for the `CLOCK_SET` round-trip.
const SET_EPOCH: i64 = 1_600_000_000;

fn main(_args: raven_rt::Args) -> i32 {
    // Wall clock: plausible and monotonic.
    let first = clock_get(clock::REALTIME);
    let second = clock_get(clock::REALTIME);
    let (first, second) = match (first, second) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            println!("time-entropy: fail clock_get");
            return 1;
        }
    };
    if !(MIN_EPOCH..=MAX_EPOCH).contains(&first.tv_sec) {
        println!(
            "time-entropy: fail realtime implausible sec={}",
            first.tv_sec
        );
        return 1;
    }
    if second.tv_sec < first.tv_sec
        || (second.tv_sec == first.tv_sec && second.tv_nsec < first.tv_nsec)
    {
        println!("time-entropy: fail realtime went backwards");
        return 1;
    }
    println!("time-entropy: realtime ok sec={}", first.tv_sec);

    // CLOCK_SET round-trip.
    let target = Timespec {
        tv_sec: SET_EPOCH,
        tv_nsec: 0,
    };
    // SAFETY: `clock_set` reads one `Timespec` from the valid `&target` pointer.
    let set = unsafe {
        syscall2(
            CLOCK_SET,
            clock::REALTIME as usize,
            &target as *const Timespec as usize,
        )
    };
    if set != 0 {
        println!("time-entropy: fail clock_set returned {set}");
        return 1;
    }
    match clock_get(clock::REALTIME) {
        Ok(now) if now.tv_sec >= SET_EPOCH => println!("time-entropy: clock_set ok"),
        Ok(now) => {
            println!("time-entropy: fail clock_set readback sec={}", now.tv_sec);
            return 1;
        }
        Err(_) => {
            println!("time-entropy: fail clock_get after set");
            return 1;
        }
    }

    // GETRANDOM.
    let mut first_draw = [0u8; 32];
    let mut second_draw = [0u8; 32];
    // SAFETY: `getrandom` writes at most the buffer length into the valid mutable buffers.
    let n1 = unsafe {
        syscall3(
            GETRANDOM,
            first_draw.as_mut_ptr() as usize,
            first_draw.len(),
            0,
        )
    };
    // SAFETY: `getrandom` writes at most the buffer length into the valid mutable buffer.
    let n2 = unsafe {
        syscall3(
            GETRANDOM,
            second_draw.as_mut_ptr() as usize,
            second_draw.len(),
            0,
        )
    };
    if n1 != first_draw.len() as isize || n2 != second_draw.len() as isize {
        println!("time-entropy: fail getrandom short n1={n1} n2={n2}");
        return 1;
    }
    if first_draw.iter().all(|&byte| byte == 0) {
        println!("time-entropy: fail getrandom all-zero");
        return 1;
    }
    if first_draw == second_draw {
        println!("time-entropy: fail getrandom repeated");
        return 1;
    }
    println!("time-entropy: getrandom ok");

    println!("time-entropy: ok");
    0
}
