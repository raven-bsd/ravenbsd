// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K508 process-accounting fixture: `PROC_TIMES`, `PROC_GETRUSAGE`, reaped-child folding and a
//! priority change that must not corrupt the counters.

#![no_std]
#![no_main]

extern crate alloc;

#[path = "common.rs"]
mod common;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::priority::{PRIO_PROCESS, nice, setpriority};
use raven_rt::sys;

use common::*;

entry!(main);

const MODE_BUSY: usize = 1;
const MODE_SLEEP: usize = 2;
const BUSY_ITERS: u64 = 30_000_000;
const BUSY_MIN_NS: u64 = 1_000_000;
const SLEEP_NS_MAX: u64 = 20_000_000;
/// Pages the busy child touches before it exits without calling `getrusage`.
const RSS_PAGES: usize = 256;

fn spin(iterations: u64) -> u64 {
    let mut value = 0x9e37_79b9_7f4a_7c15u64;
    for _ in 0..iterations {
        value = value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    }
    core::hint::black_box(value)
}

extern "C" fn child_entry(mode: usize) -> ! {
    if mode == MODE_BUSY {
        // Known resident footprint for the parent's RUSAGE_CHILDREN maxrss assertion. The child
        // must not call getrusage: only the at-exit snapshot can fold this footprint.
        let mut block: alloc::vec::Vec<u8> = alloc::vec![0u8; RSS_PAGES * PAGE];
        for page in 0..RSS_PAGES {
            block[page * PAGE] = (page as u8).wrapping_add(1);
        }
        let _ = spin(BUSY_ITERS);
        core::hint::black_box(&block);
        sys::exit(7);
    }
    if mode == MODE_SLEEP {
        let _ = sys::nanosleep(200_000_000);
        sys::exit(0);
    }
    sys::exit(99)
}

fn fork_child(mode: usize) -> Result<usize, Errno> {
    let child = proc_fork(Handle::SELF_PROC)?;
    let pid = handle_info(child)?;
    thread_create(child, child_entry as *const () as usize, stack_top(), mode)?;
    Ok(pid)
}

fn wait_status(pid: i32) -> Result<i32, Errno> {
    let mut status = 0i32;
    wait_ex(P_PID, pid, 0, &mut status)?;
    Ok(status)
}

fn exit_code(status: i32) -> i32 {
    (status >> 8) & 0xff
}

fn main(_args: raven_rt::Args) -> i32 {
    let before_self = match times() {
        Ok(value) => value,
        Err(_) => {
            println!("proc-times: fail self-times");
            return 1;
        }
    };
    let before_rusage_self = match getrusage() {
        Ok(value) => value,
        Err(_) => {
            println!("proc-times: fail self-rusage");
            return 1;
        }
    };
    let _ = spin(20_000_000);
    let after_self = match times() {
        Ok(value) => value,
        Err(_) => {
            println!("proc-times: fail self-times-after");
            return 1;
        }
    };
    let after_rusage_self = match getrusage() {
        Ok(value) => value,
        Err(_) => {
            println!("proc-times: fail self-rusage-after");
            return 1;
        }
    };
    let self_user = after_self[0].saturating_sub(before_self[0]);
    let self_system = after_self[1].saturating_sub(before_self[1]);
    let monotonic = after_self[0] >= before_self[0]
        && after_self[1] >= before_self[1]
        && after_self[2] >= before_self[2]
        && after_self[3] >= before_self[3];
    let rusage_monotonic = after_rusage_self[0] >= before_rusage_self[0]
        && after_rusage_self[1] >= before_rusage_self[1];
    if !monotonic
        || !rusage_monotonic
        || self_user.saturating_add(self_system) == 0
        || after_rusage_self[0].saturating_add(after_rusage_self[1])
            < before_rusage_self[0].saturating_add(before_rusage_self[1])
    {
        println!(
            "proc-times: fail self-account user={self_user} system={self_system} monotonic={}",
            u8::from(monotonic)
        );
        return 1;
    }
    println!("proc-times: self-user=1 self-system=1");

    let before_busy = match times() {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let before_children = match getrusage_who(-1) {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let busy = match fork_child(MODE_BUSY) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-times: fail busy-fork");
            return 1;
        }
    };
    let busy_status = match wait_status(busy as i32) {
        Ok(status) => status,
        Err(_) => {
            println!("proc-times: fail busy-wait");
            return 1;
        }
    };
    let after_busy = match times() {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let after_children = match getrusage_who(-1) {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let child_user = after_busy[2].saturating_sub(before_busy[2]);
    let child_system = after_busy[3].saturating_sub(before_busy[3]);
    let child_rusage_user = after_children[0].saturating_sub(before_children[0]);
    let child_maxrss = after_children[2];
    if exit_code(busy_status) != 7
        || child_user.saturating_add(child_system) < BUSY_MIN_NS
        || child_rusage_user < BUSY_MIN_NS
        || child_maxrss < (RSS_PAGES * PAGE) as u64
    {
        println!(
            "proc-times: fail busy child exit={} user={child_user} system={child_system} rusage={child_rusage_user} maxrss={child_maxrss}",
            exit_code(busy_status)
        );
        return 1;
    }
    println!("proc-times: busy-child=1 child-user-ns={child_user} child-system-ns={child_system}");

    let before_sleep = match times() {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let sleeper = match fork_child(MODE_SLEEP) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-times: fail sleep-fork");
            return 1;
        }
    };
    let sleep_status = match wait_status(sleeper as i32) {
        Ok(status) => status,
        Err(_) => {
            println!("proc-times: fail sleep-wait");
            return 1;
        }
    };
    let after_sleep = match times() {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let sleep_user = after_sleep[2].saturating_sub(before_sleep[2]);
    if exit_code(sleep_status) != 0 || sleep_user >= SLEEP_NS_MAX {
        println!(
            "proc-times: fail sleep child exit={} user={sleep_user}",
            exit_code(sleep_status)
        );
        return 1;
    }
    println!("proc-times: sleep-child=1 sleep-user-ns={sleep_user}");

    let before_priority = match times() {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let priority_ok = setpriority(PRIO_PROCESS, 0, 5).is_ok()
        && nice(1).is_ok()
        && setpriority(PRIO_PROCESS, 0, 0).is_ok();
    let after_priority = match times() {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let priority_monotonic = after_priority[0] >= before_priority[0]
        && after_priority[1] >= before_priority[1]
        && after_priority[2] >= before_priority[2]
        && after_priority[3] >= before_priority[3];
    if !priority_ok || !priority_monotonic {
        println!("proc-times: fail priority accounting");
        return 1;
    }
    println!("proc-times: priority=1 accounting=1");
    println!("proc-times: reaped-fold=1 maxrss={child_maxrss}");
    println!("proc-times: cpuclock=not-exposed");
    println!("proc-times: ok");
    0
}
