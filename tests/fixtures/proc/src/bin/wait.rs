// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel wait and orphan-reparenting fixture.
//!
//! `/init` runs the wait matrix and then spawns its own image in the child modes. The child
//! modes exit/sleep/loop through the kernel `EXIT` and `PROC_*` calls; the orphan-parent mode
//! exits while its own child is still alive so the child is reparented to PID 1.

#![no_std]
#![no_main]

#[path = "../common.rs"]
mod common;

use raven_abi::errno::Errno;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};

use common::*;

entry!(main);

const EXIT_7: i32 = 7 << 8;
const EXIT_3: i32 = 3 << 8;
const EXIT_4: i32 = 4 << 8;
const EXIT_5: i32 = 5 << 8;
const STOP_STATUS: i32 = (19 << 8) | 0x7f;
const CONTINUED_STATUS: i32 = 0xffff;
const KILL_STATUS: i32 = 137 << 8;

fn main(args: raven_rt::Args) -> i32 {
    match args.get(1) {
        Some("exit") => child_exit(&args),
        Some("sleep-exit") => child_sleep_exit(&args),
        Some("stop-wait") => child_stop_wait(),
        Some("orphan-parent") => child_orphan_parent(),
        Some("orphan-child") => child_orphan(),
        _ => match wait_cases() {
            Ok(()) => {
                println!("proc-wait: ok");
                0
            }
            Err(step) => {
                println!("proc-wait: fail {step}");
                1
            }
        },
    }
}

fn wait_cases() -> Result<(), &'static str> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;

    let child = spawn_child(&initfs, &["proc-wait", "exit", "7"])?;
    let (pid, status) = wait_status(P_PID, child, 0).map_err(|_| "waitpid")?;
    if pid != child as usize || status != EXIT_7 {
        println!("proc-wait: waitpid-debug child={child} pid={pid} status={status:#x}");
        return Err("waitpid status");
    }
    if wait_status(P_PID, child, 0) != Err(Errno::ECHILD) {
        return Err("double wait");
    }
    println!("proc-wait: waitpid=1 echild=1");

    let first = spawn_child(&initfs, &["proc-wait", "exit", "3"])?;
    let second = spawn_child(&initfs, &["proc-wait", "exit", "4"])?;
    let (pid_a, status_a) = wait_status(P_ALL, 0, 0).map_err(|_| "wait-any-a")?;
    let (pid_b, status_b) = wait_status(P_ALL, 0, 0).map_err(|_| "wait-any-b")?;
    let any_ok = (pid_a == first as usize
        && status_a == EXIT_3
        && pid_b == second as usize
        && status_b == EXIT_4)
        || (pid_a == second as usize
            && status_a == EXIT_4
            && pid_b == first as usize
            && status_b == EXIT_3);
    if !any_ok {
        return Err("wait-any status");
    }
    if wait_status(P_ALL, 0, WNOHANG) != Err(Errno::ECHILD) {
        return Err("wait-any echild");
    }
    println!("proc-wait: wait-any=1");

    let child = spawn_child(&initfs, &["proc-wait", "sleep-exit", "5", "300"])?;
    if wait_status(P_PID, child, WNOHANG) != Ok((0, 0)) {
        return Err("nohang");
    }
    let (pid, status) = wait_status(P_PID, child, 0).map_err(|_| "nohang wait")?;
    if pid != child as usize || status != EXIT_5 {
        return Err("nohang status");
    }
    println!("proc-wait: nohang=1");

    let child = spawn_child(&initfs, &["proc-wait", "stop-wait"])?;
    signal(child, SIGSTOP).map_err(|_| "stop")?;
    let (pid, status) = wait_status(P_PID, child, WUNTRACED).map_err(|_| "wait-stop")?;
    if pid != child as usize || status != STOP_STATUS {
        return Err("stop status");
    }
    signal(child, SIGCONT).map_err(|_| "cont")?;
    let (pid, status) = wait_status(P_PID, child, WCONTINUED).map_err(|_| "wait-cont")?;
    if pid != child as usize || status != CONTINUED_STATUS {
        return Err("cont status");
    }
    signal(child, SIGKILL).map_err(|_| "kill")?;
    let (pid, status) = wait_status(P_PID, child, 0).map_err(|_| "wait-kill")?;
    if pid != child as usize || status != KILL_STATUS {
        return Err("kill status");
    }
    println!("proc-wait: stop=1");

    let parent = spawn_child(&initfs, &["proc-wait", "orphan-parent"])?;
    let (pid, status) = wait_status(P_PID, parent, 0).map_err(|_| "orphan parent")?;
    if pid != parent as usize {
        return Err("orphan parent pid");
    }
    let child_pid = (status >> 8) & 0xff;
    if child_pid <= 0 {
        return Err("orphan child pid");
    }
    let info = pid_info(child_pid).map_err(|_| "orphan info")?;
    if info[1] != 1 {
        return Err("orphan not reparented");
    }
    let (pid, status) = wait_status(P_ALL, 0, 0).map_err(|_| "orphan wait")?;
    if pid != child_pid as usize || status != 0 {
        return Err("orphan wait status");
    }
    println!("proc-wait: reparent=1");
    Ok(())
}

fn spawn_child(initfs: &Initfs<'_>, args: &[&str]) -> Result<i32, &'static str> {
    let handle =
        spawn(initfs, "/init", &SpawnOptions { args, env: &[] }).map_err(|_| "spawn child")?;
    let pid = handle_info(handle).map_err(|_| "child pid")?;
    let _ = close(handle);
    i32::try_from(pid).map_err(|_| "child pid range")
}

fn wait_status(idtype: usize, id: i32, options: usize) -> Result<(usize, i32), Errno> {
    let mut status = 0i32;
    wait_ex(idtype, id, options, &mut status).map(|pid| (pid, status))
}

fn child_exit(args: &raven_rt::Args) -> i32 {
    args.get(2)
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(1)
}

fn child_sleep_exit(args: &raven_rt::Args) -> i32 {
    let code = args
        .get(2)
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(1);
    let millis = args
        .get(3)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(100);
    let _ = raven_rt::nanosleep(millis.saturating_mul(1_000_000));
    code
}

fn child_stop_wait() -> i32 {
    loop {
        let _ = raven_rt::nanosleep(10_000_000);
    }
}

fn child_orphan_parent() -> i32 {
    let Ok(image) = raven_rt::map_initfs() else {
        return 1;
    };
    let Ok(initfs) = Initfs::parse(image) else {
        return 2;
    };
    let Ok(handle) = spawn(
        &initfs,
        "/init",
        &SpawnOptions {
            args: &["proc-wait", "orphan-child"],
            env: &[],
        },
    ) else {
        return 3;
    };
    let Ok(pid) = handle_info(handle) else {
        return 4;
    };
    let _ = close(handle);
    i32::try_from(pid).unwrap_or(5)
}

fn child_orphan() -> i32 {
    let _ = raven_rt::nanosleep(500_000_000);
    0
}

#[cfg(not(target_os = "none"))]
fn main() {}
