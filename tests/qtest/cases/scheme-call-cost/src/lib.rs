// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K147 fixture helpers: positional reads, a monotonic clock and the p50/p90 report.

#![no_std]
#![allow(unsafe_code)] // Raw syscall assembly and mapped benchmark buffers; every block has SAFETY.

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Timespec, clock};
use raven_rt::println;
use raven_s11_echo::{result, syscall6};

/// RAW `pread(handle, buffer, len, offset)` syscall number (PG-22).
pub const PREAD: usize = 0x6b;

/// Positional read from a scheme file.
pub fn pread(handle: Handle, buffer: &mut [u8], offset: u64) -> Result<usize, Errno> {
    result(syscall6(
        PREAD,
        handle.0 as usize,
        buffer.as_mut_ptr() as usize,
        buffer.len(),
        offset as usize,
        0,
        0,
    ))
}

/// Monotonic time in nanoseconds.
pub fn now_ns() -> u64 {
    let mut time = Timespec::default();
    let _ = result(syscall6(
        Syscall::CLOCK_GET as usize,
        clock::MONOTONIC as usize,
        &mut time as *mut Timespec as usize,
        0,
        0,
        0,
        0,
    ));
    u64::try_from(time.tv_sec)
        .unwrap_or(0)
        .saturating_mul(1_000_000_000)
        .saturating_add(u64::try_from(time.tv_nsec).unwrap_or(0))
}

/// Sort `samples` in place with an insertion sort (fixed, allocation-free).
pub fn sort(samples: &mut [u64]) {
    for index in 1..samples.len() {
        let value = samples[index];
        let mut position = index;
        while position > 0 && samples[position - 1] > value {
            samples[position] = samples[position - 1];
            position -= 1;
        }
        samples[position] = value;
    }
}

/// Print every raw sample and the p50/p90 summary for one operation.
pub fn report(name: &str, samples: &mut [u64]) -> u64 {
    let count = samples.len();
    for (index, value) in samples.iter().enumerate() {
        println!("scheme-call-cost: raw {} n={} ns={}", name, index, value);
    }
    sort(samples);
    let p50 = samples
        .get(count.saturating_mul(50) / 100)
        .copied()
        .unwrap_or(0);
    let p90 = samples
        .get(count.saturating_mul(90) / 100)
        .copied()
        .unwrap_or(0);
    println!(
        "scheme-call-cost: {} n={} p50_ns={} p90_ns={}",
        name, count, p50, p90
    );
    p50
}
