// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K529: only euid 0 may set the wall clock or control global DTrace.
//!
//! Root first proves the operations work; the process then drops to uid 1000 and every control
//! must return EPERM while CLOCK_GET stays at the root-set value. No DTrace record is readable.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Timespec, clock};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::proc::{setresgid, setresuid};
use raven_rt::sys::{clock_get, syscall2, syscall6};

entry!(main);

const CLOCK_SET: usize = 0x09;
const PROBE_LIST: usize = 0x90;
const ENABLE: usize = 0x91;
const DISABLE: usize = 0x92;
const READ: usize = 0x93;
const STATS: usize = 0x94;
const CLEAR: usize = 0x95;
const PROBE: usize = 1;

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("capability: ok");
            0
        }
        Err(message) => {
            println!("capability: fail {message}");
            1
        }
    }
}

fn run() -> Result<(), String> {
    let base = clock_get(clock::REALTIME)
        .map_err(|error| alloc::format!("clock get: {}", error.description()))?;
    let target = Timespec {
        tv_sec: base.tv_sec.saturating_add(100),
        tv_nsec: 0,
    };
    raw_clock_set(&target)
        .map_err(|error| alloc::format!("root clock set: {}", error.description()))?;
    let after = clock_get(clock::REALTIME)
        .map_err(|error| alloc::format!("clock get after: {}", error.description()))?;
    if after.tv_sec < target.tv_sec {
        return Err("root clock did not move".into());
    }
    let mut list = [0u8; 4096];
    if dtrace(PROBE_LIST, list.as_mut_ptr() as usize, 4).is_err() {
        return Err("root probe list".into());
    }
    if let Err(error) = dtrace3(ENABLE, PROBE, 0, Handle::NONE.0 as usize) {
        return Err(alloc::format!("root enable {}", error.description()));
    }
    if let Err(error) = dtrace(DISABLE, PROBE, 0) {
        return Err(alloc::format!("root disable {}", error.description()));
    }
    let mut stats = [0u8; 4096];
    if dtrace(STATS, stats.as_mut_ptr() as usize, 16).is_err() {
        return Err("root stats".into());
    }
    if dtrace(READ, 0, 0).is_err() || dtrace(CLEAR, 0, 0).is_err() {
        return Err("root read/clear".into());
    }
    println!("capability: root clock=1 dtrace=1");

    setresgid(1000).map_err(|error| alloc::format!("setresgid: {}", error.description()))?;
    setresuid(1000).map_err(|error| alloc::format!("setresuid: {}", error.description()))?;

    let denied = raw_clock_set(&Timespec {
        tv_sec: target.tv_sec.saturating_add(100),
        tv_nsec: 0,
    }) == Err(Errno::EPERM);
    let unchanged = clock_get(clock::REALTIME).is_ok_and(|now| now.tv_sec == after.tv_sec);
    let mut list = [0u8; 256];
    let dtrace_denied = dtrace(PROBE_LIST, list.as_mut_ptr() as usize, 4) == Err(Errno::EPERM)
        && dtrace3(ENABLE, PROBE, 0, Handle::NONE.0 as usize) == Err(Errno::EPERM)
        && dtrace(DISABLE, PROBE, 0) == Err(Errno::EPERM)
        && dtrace(READ, list.as_mut_ptr() as usize, 1) == Err(Errno::EPERM)
        && dtrace(STATS, list.as_mut_ptr() as usize, 1) == Err(Errno::EPERM)
        && dtrace(CLEAR, 0, 0) == Err(Errno::EPERM);
    if !(denied && unchanged && dtrace_denied) {
        return Err(alloc::format!(
            "nonroot clock={denied} unchanged={unchanged} dtrace={dtrace_denied}"
        ));
    }
    println!("capability: nonroot clock=1 unchanged=1 dtrace=1");
    Ok(())
}

fn raw_clock_set(timespec: &Timespec) -> Result<usize, Errno> {
    // SAFETY: CLOCK_SET reads one Timespec from the caller's live reference.
    let raw = unsafe {
        syscall2(
            CLOCK_SET,
            clock::REALTIME as usize,
            timespec as *const Timespec as usize,
        )
    };
    Errno::from_ret(raw)
}

fn dtrace(number: usize, a0: usize, a1: usize) -> Result<usize, Errno> {
    // SAFETY: the DTrace syscalls take scalar arguments; the kernel validates the pointer and
    // the length before touching the user buffer.
    let raw = unsafe { syscall6(number, a0, a1, 0, 0, 0, 0) };
    Errno::from_ret(raw)
}

fn dtrace3(number: usize, a0: usize, a1: usize, a2: usize) -> Result<usize, Errno> {
    // SAFETY: ENABLE carries probe, flags and the optional event handle as scalars.
    let raw = unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) };
    Errno::from_ret(raw)
}
