// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2l priority fixture: root get/set/clamp/nice, group/user paths, scheduler queries, then an
//! unprivileged phase after dropping to uid 1000.

#![no_std]
#![no_main]

#[path = "../common.rs"]
mod common;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::priority::{
    NICE_MAX, NICE_MIN, PRIO_GROUP, PRIO_PROCESS, PRIO_USER, SCHED_FIFO, SCHED_OTHER, SCHED_RR,
    getpriority, nice, sched_get_priority_max, sched_get_priority_min, sched_getscheduler,
    sched_setscheduler, setpriority,
};
use raven_rt::proc::{setresgid, setresuid};
use raven_rt::sys::proc_terminate;

use common::*;

entry!(main);

fn proc_create() -> Result<Handle, Errno> {
    checked(unsafe { syscall6(Syscall::PROC_CREATE as usize, 0, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

struct Root {
    get: bool,
    set: bool,
    clamp: bool,
    nice: bool,
    invalid: bool,
    group: bool,
    user: bool,
    sched: bool,
}

fn root_checks() -> Result<Root, Errno> {
    let get = getpriority(PRIO_PROCESS, 0)? == 0;
    setpriority(PRIO_PROCESS, 0, 5)?;
    let set = getpriority(PRIO_PROCESS, 0)? == 5;
    setpriority(PRIO_PROCESS, 0, 100)?;
    let high = getpriority(PRIO_PROCESS, 0)? == NICE_MAX;
    setpriority(PRIO_PROCESS, 0, -100)?;
    let low = getpriority(PRIO_PROCESS, 0)? == NICE_MIN;
    let clamp = high && low;
    setpriority(PRIO_PROCESS, 0, 0)?;
    let nice = nice(5)? == 5 && nice(-100)? == NICE_MIN;
    setpriority(PRIO_PROCESS, 0, 0)?;
    let invalid = getpriority(7, 0) == Err(Errno::EINVAL)
        && getpriority(PRIO_PROCESS, 999_999) == Err(Errno::ESRCH)
        && setpriority(7, 0, 0) == Err(Errno::EINVAL);

    setpriority(PRIO_GROUP, 0, 3)?;
    let group = getpriority(PRIO_GROUP, 0)? == 3;
    setpriority(PRIO_GROUP, 0, 0)?;
    setpriority(PRIO_USER, 0, 4)?;
    let user = getpriority(PRIO_USER, 0)? == 4;
    setpriority(PRIO_USER, 0, 0)?;

    let sched = sched_get_priority_max(SCHED_OTHER)? == 0
        && sched_get_priority_min(SCHED_OTHER)? == 0
        && sched_get_priority_max(SCHED_FIFO) == Err(Errno::ENOTSUP)
        && sched_get_priority_min(SCHED_RR) == Err(Errno::ENOTSUP)
        && sched_get_priority_max(9) == Err(Errno::EINVAL)
        && sched_getscheduler(0)? == SCHED_OTHER as i32
        && sched_setscheduler(0, SCHED_OTHER, 0).is_ok()
        && sched_setscheduler(0, SCHED_OTHER, 1) == Err(Errno::EINVAL)
        && sched_setscheduler(0, SCHED_FIFO, 0) == Err(Errno::ENOTSUP);

    Ok(Root {
        get,
        set,
        clamp,
        nice,
        invalid,
        group,
        user,
        sched,
    })
}

struct User {
    get: bool,
    set: bool,
    raise: bool,
    nice: bool,
    path: bool,
    other: bool,
    group: bool,
    sched: bool,
}

fn user_checks(child_pid: i32) -> Result<User, Errno> {
    let get = getpriority(PRIO_PROCESS, 0)? == 0;
    setpriority(PRIO_PROCESS, 0, 10)?;
    let set = getpriority(PRIO_PROCESS, 0)? == 10;
    let raise = setpriority(PRIO_PROCESS, 0, -5) == Err(Errno::EPERM)
        && getpriority(PRIO_PROCESS, 0)? == 10;
    let nice = nice(5)? == 15 && nice(-1)? == 15;
    let path = setpriority(PRIO_USER, 0, 18).is_ok() && getpriority(PRIO_USER, 0)? == 18;
    let other = setpriority(PRIO_PROCESS, child_pid, 10) == Err(Errno::EPERM)
        && getpriority(PRIO_PROCESS, child_pid) == Err(Errno::EPERM);
    let group = setpriority(PRIO_GROUP, 0, 10) == Err(Errno::EPERM)
        && getpriority(PRIO_GROUP, 0) == Err(Errno::EPERM);
    let sched = sched_setscheduler(child_pid, SCHED_OTHER, 0) == Err(Errno::EPERM)
        && sched_getscheduler(child_pid) == Err(Errno::EPERM);
    Ok(User {
        get,
        set,
        raise,
        nice,
        path,
        other,
        group,
        sched,
    })
}

fn run() -> Result<(), Errno> {
    let root = root_checks()?;
    let child = proc_create()?;
    let child_pid = i32::try_from(handle_info(child)?).map_err(|_| Errno::EINVAL)?;

    setresgid(1000)?;
    setresuid(1000)?;
    let user = user_checks(child_pid)?;

    let _ = proc_terminate(child, 0);
    let mut status = 0i32;
    let _ = wait_ex(P_PID, child_pid, 0, &mut status);
    let _ = close(child);

    println!(
        "proc-priority: root get={} set={} clamp={} nice={} invalid={}",
        u8::from(root.get),
        u8::from(root.set),
        u8::from(root.clamp),
        u8::from(root.nice),
        u8::from(root.invalid)
    );
    println!(
        "proc-priority: root group={} user={} sched={}",
        u8::from(root.group),
        u8::from(root.user),
        u8::from(root.sched)
    );
    println!(
        "proc-priority: user get={} set={} raise={} nice={}",
        u8::from(user.get),
        u8::from(user.set),
        u8::from(user.raise),
        u8::from(user.nice)
    );
    println!(
        "proc-priority: user path={} other={} group={} sched={}",
        u8::from(user.path),
        u8::from(user.other),
        u8::from(user.group),
        u8::from(user.sched)
    );
    let ok = root.get
        && root.set
        && root.clamp
        && root.nice
        && root.invalid
        && root.group
        && root.user
        && root.sched
        && user.get
        && user.set
        && user.raise
        && user.nice
        && user.path
        && user.other
        && user.group
        && user.sched;
    if !ok {
        println!("proc-priority: FAIL checks");
        return Err(Errno::EINVAL);
    }
    println!("proc-priority: ok");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(error) => {
            println!("proc-priority: error {error:?}");
            1
        }
    }
}
