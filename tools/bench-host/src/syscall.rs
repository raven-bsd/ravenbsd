// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw syscall round-trip measurements.

use std::fs::File;
use std::os::fd::AsRawFd;

use anyhow::{Context, Result};
use libc::c_void;

use crate::affinity::pin_first_cpu;
use crate::measure::measure;
use crate::result::BenchResult;

const RUNS: usize = 30;
const ITERATIONS: usize = 100_000;
const WARMUP: usize = 10_000;

pub(crate) fn run() -> Result<()> {
    let _affinity = pin_first_cpu()?;

    let getpid = measure(RUNS, ITERATIONS, WARMUP, || {
        // SAFETY: SYS_getpid takes no arguments and has no memory side effects.
        unsafe {
            libc::syscall(libc::SYS_getpid);
        }
    });
    emit("syscall-rtt-getpid", "ns/op", &getpid)?;

    let mut timespec = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let clock = measure(RUNS, ITERATIONS, WARMUP, || {
        // SAFETY: SYS_clock_gettime reads CLOCK_MONOTONIC into the writable `timespec`.
        unsafe {
            libc::syscall(
                libc::SYS_clock_gettime,
                libc::CLOCK_MONOTONIC,
                &mut timespec as *mut libc::timespec,
            );
        }
    });
    emit("syscall-rtt-clock_gettime", "ns/op", &clock)?;

    let zero = File::open("/dev/zero").context("cannot open /dev/zero")?;
    let fd = zero.as_raw_fd();
    let mut byte = 0u8;
    let read = measure(RUNS, ITERATIONS, WARMUP, || {
        // SAFETY: `fd` is an open file descriptor and `byte` is a writable one-byte buffer.
        unsafe {
            libc::read(fd, &mut byte as *mut u8 as *mut c_void, 1);
        }
    });
    emit("syscall-rtt-read-zero", "ns/op", &read)?;

    Ok(())
}

fn emit(name: &str, unit: &str, samples: &[f64]) -> Result<()> {
    let result = BenchResult::from_samples(name, unit, samples, None);
    result.print();
    result.write()?;
    Ok(())
}
