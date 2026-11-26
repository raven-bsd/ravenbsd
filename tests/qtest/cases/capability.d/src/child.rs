// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K529 uid-1000 child: CLOCK_SET and every DTrace control must return EPERM without touching
//! the clock or a ring; the read-only probe list stays available and exposes no records.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_capability::{
    CpuStats, DTRACE_CLEAR, DTRACE_DISABLE, DTRACE_ENABLE, DTRACE_PROBE_LIST, DTRACE_READ,
    DTRACE_STATS, Record, SYSCALL_PROBE, clock_get, clock_set, dtrace,
};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const UID: u32 = 1000;
const CHILD_SECONDS: i64 = 1_600_000_000;

fn fail(message: &str) -> i32 {
    println!("capability: fail {message}");
    1
}

fn expect_eperm(label: &str, result: Result<usize, Errno>) -> Result<(), ()> {
    match result {
        Err(Errno::EPERM) => Ok(()),
        _ => {
            println!("capability: fail child {label} accepted");
            Err(())
        }
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    if raven_rt::proc::setresuid(UID).is_err() {
        return fail("child setuid");
    }
    let credentials = match raven_rt::proc::credentials() {
        Ok(credentials) => credentials,
        Err(_) => return fail("child credentials"),
    };
    if credentials.euid != UID {
        println!("capability: fail child euid {}", credentials.euid);
        return 1;
    }

    let before = match clock_get() {
        Ok(value) => value,
        Err(_) => return fail("child clock read before"),
    };
    if !matches!(clock_set(CHILD_SECONDS, 0), Err(Errno::EPERM)) {
        return fail("child clock accepted");
    }
    let after = match clock_get() {
        Ok(value) => value,
        Err(_) => return fail("child clock read after"),
    };
    // The denied set would jump the clock by 100M seconds; a live clock only drifts by
    // nanoseconds between the two reads.
    if before.tv_sec.abs_diff(after.tv_sec) > 1 || after.tv_sec == CHILD_SECONDS {
        println!(
            "capability: fail child clock changed {} -> {}",
            before.tv_sec, after.tv_sec
        );
        return 1;
    }
    println!("capability: child clock eperm ok");

    let mut records = [Record::default(); 1];
    let mut stats = [CpuStats::default(); 1];
    let controls = [
        (
            "enable",
            dtrace(
                DTRACE_ENABLE,
                SYSCALL_PROBE as usize,
                0,
                raven_abi::types::Handle::NONE.0 as usize,
            ),
        ),
        (
            "disable",
            dtrace(DTRACE_DISABLE, SYSCALL_PROBE as usize, 0, 0),
        ),
        (
            "read",
            dtrace(
                DTRACE_READ,
                records.as_mut_ptr() as usize,
                records.len(),
                0,
            ),
        ),
        (
            "stats",
            dtrace(DTRACE_STATS, stats.as_mut_ptr() as usize, stats.len(), 0),
        ),
        ("clear", dtrace(DTRACE_CLEAR, 0, 0, 0)),
    ];
    for (label, result) in controls {
        if expect_eperm(label, result).is_err() {
            return 1;
        }
    }
    println!("capability: child dtrace eperm ok");

    // The probe list is metadata only: allowed for uid 1000, no cross-process record.
    match dtrace(DTRACE_PROBE_LIST, 0, 0, 0) {
        Ok(1) => println!("capability: child probe-list ok"),
        Ok(count) => {
            println!("capability: fail child probe-list count {count}");
            return 1;
        }
        Err(error) => {
            println!("capability: fail child probe-list {}", error.0);
            return 1;
        }
    }

    println!("capability: child ok");
    0
}
