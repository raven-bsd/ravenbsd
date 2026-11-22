// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K494 orphan reparent/reap contract fixture.

#![no_std]
#![no_main]

#[path = "common.rs"]
mod common;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

use common::*;

entry!(main);

const CONTROL: usize = 0x4000_0000;
const PROC_SET_COMMAND: usize = 0x45;

const SLOT_P: usize = 0;
const SLOT_O: usize = 1;
const SLOT_G: usize = 2;
const SLOT_O_READY: usize = 3;
const SLOT_G_READY: usize = 4;
const SLOT_O_PPID: usize = 5;
const SLOT_O_EXIT: usize = 6;
const SLOT_G_EXIT: usize = 7;
const SLOT_P2: usize = 8;
const SLOT_BATCH_EXIT: usize = 9;
const BATCH_BASE: usize = 10;
const BATCH_READY: usize = 20;
const BATCH_COUNT: usize = 10;

const MODE_P: usize = 1;
const MODE_O: usize = 2;
const MODE_G: usize = 3;
const MODE_BATCH_PARENT: usize = 4;
const MODE_BATCH_WORKER: usize = 5;
const MODE_PROBE: usize = 6;

fn slot(index: usize) -> usize {
    CONTROL + index * 8
}

fn wait_slot(index: usize) -> Option<u64> {
    for _ in 0..5000 {
        let value = read64(slot(index));
        if value != 0 {
            return Some(value);
        }
        sleep_ms();
    }
    None
}

fn set_command() {
    let name = b"s69-orphan";
    let _ = checked(unsafe {
        sys::syscall6(
            PROC_SET_COMMAND,
            Handle::SELF_PROC.0 as usize,
            name.as_ptr() as usize,
            name.len(),
            0,
            0,
            0,
        )
    });
}

fn fork_thread(mode: usize, index: usize) -> Result<usize, Errno> {
    let child = proc_fork(Handle::SELF_PROC)?;
    let pid = handle_info(child)?;
    let argument = (index << 8) | mode;
    thread_create(
        child,
        child_entry as *const () as usize,
        stack_top(),
        argument,
    )?;
    Ok(pid)
}

extern "C" fn child_entry(argument: usize) -> ! {
    set_command();
    let mode = argument & 0xff;
    let index = argument >> 8;
    match mode {
        MODE_P => parent_entry(),
        MODE_O => orphan_entry(),
        MODE_G => grandchild_entry(),
        MODE_BATCH_PARENT => batch_parent_entry(),
        MODE_BATCH_WORKER => batch_worker_entry(index),
        MODE_PROBE => sys::exit(0),
        _ => sys::exit(99),
    }
}

fn parent_entry() -> ! {
    let Ok(info) = self_info() else { sys::exit(90) };
    write64(slot(SLOT_P), info[0]);
    let _o = match fork_thread(MODE_O, 0) {
        Ok(pid) => pid,
        Err(_) => sys::exit(91),
    };
    if wait_slot(SLOT_O_READY).is_none() || wait_slot(SLOT_G_READY).is_none() {
        sys::exit(92);
    }
    sys::exit(5)
}

fn orphan_entry() -> ! {
    let Ok(info) = self_info() else { sys::exit(90) };
    write64(slot(SLOT_O), info[0]);
    write64(slot(SLOT_O_READY), 1);
    if fork_thread(MODE_G, 0).is_err() {
        sys::exit(93);
    }
    if wait_slot(SLOT_G_READY).is_none() {
        sys::exit(94);
    }
    if let Ok(now) = self_info() {
        write64(slot(SLOT_O_PPID), now[1]);
    }
    loop {
        let code = read64(slot(SLOT_O_EXIT));
        if code != 0 {
            sys::exit(code as i32);
        }
        sleep_ms();
    }
}

fn grandchild_entry() -> ! {
    let Ok(info) = self_info() else { sys::exit(90) };
    write64(slot(SLOT_G), info[0]);
    write64(slot(SLOT_G_READY), 1);
    loop {
        let code = read64(slot(SLOT_G_EXIT));
        if code != 0 {
            sys::exit(code as i32);
        }
        sleep_ms();
    }
}

fn batch_parent_entry() -> ! {
    let Ok(info) = self_info() else { sys::exit(90) };
    write64(slot(SLOT_P2), info[0]);
    for index in 0..BATCH_COUNT {
        if fork_thread(MODE_BATCH_WORKER, index).is_err() {
            sys::exit(95);
        }
    }
    for index in 0..BATCH_COUNT {
        if wait_slot(BATCH_READY + index).is_none() {
            sys::exit(96);
        }
    }
    sys::exit(5)
}

fn batch_worker_entry(index: usize) -> ! {
    let Ok(info) = self_info() else { sys::exit(90) };
    write64(slot(BATCH_BASE + index), info[0]);
    write64(slot(BATCH_READY + index), 1);
    loop {
        let code = read64(slot(SLOT_BATCH_EXIT));
        if code != 0 {
            sys::exit(code as i32);
        }
        sleep_ms();
    }
}

fn probe() -> bool {
    let Ok(pid) = fork_thread(MODE_PROBE, 0) else {
        return false;
    };
    match wait_status(P_PID, pid as i32, 0) {
        Ok((waited, status)) if waited == pid && exit_code(status) == 0 => true,
        _ => false,
    }
}

fn wait_status(idtype: usize, id: i32, options: usize) -> Result<(usize, i32), Errno> {
    let mut status = 0i32;
    wait_ex(idtype, id, options, &mut status).map(|pid| (pid, status))
}

fn exit_code(status: i32) -> i32 {
    (status >> 8) & 0xff
}

fn main(_args: raven_rt::Args) -> i32 {
    if control_page().is_none() {
        println!("proc-orphan: fail control");
        return 1;
    }
    if !probe() {
        println!("proc-orphan: fail baseline-probe");
        return 1;
    }
    let p = match fork_thread(MODE_P, 0) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-orphan: fail fork-parent");
            return 1;
        }
    };
    let o = match wait_slot(SLOT_O) {
        Some(pid) => pid as usize,
        None => {
            println!("proc-orphan: fail orphan-pid");
            return 1;
        }
    };
    let g = match wait_slot(SLOT_G) {
        Some(pid) => pid as usize,
        None => {
            println!("proc-orphan: fail grandchild-pid");
            return 1;
        }
    };

    match wait_status(P_PID, p as i32, 0) {
        Ok((pid, status)) if pid == p && exit_code(status) == 5 => {}
        _ => {
            println!("proc-orphan: fail parent-exit");
            return 1;
        }
    }
    let mut orphan_ppid = 0u64;
    for _ in 0..5000 {
        if let Ok(info) = pid_info(o as i32) {
            orphan_ppid = info[1];
            if orphan_ppid == 1 {
                break;
            }
        }
        sleep_ms();
    }
    if orphan_ppid != 1 {
        println!("proc-orphan: fail reparent ppid={orphan_ppid}");
        return 1;
    }
    if let Ok(info) = pid_info(g as i32)
        && info[1] != o as u64
    {
        println!("proc-orphan: fail grandchild-parent");
        return 1;
    }
    if wait_status(P_PID, g as i32, WNOHANG) != Err(Errno::ECHILD) {
        println!("proc-orphan: fail non-child");
        return 1;
    }
    if wait_status(P_PID, 9999, WNOHANG) != Err(Errno::ECHILD) {
        println!("proc-orphan: fail unknown-child");
        return 1;
    }
    if wait_status(P_PID, o as i32, WNOHANG) != Ok((0, 0)) {
        println!("proc-orphan: fail live-orphan");
        return 1;
    }

    write64(slot(SLOT_O_EXIT), 7);
    match wait_status(P_PID, o as i32, 0) {
        Ok((pid, status)) if pid == o && exit_code(status) == 7 => {}
        _ => {
            println!("proc-orphan: fail orphan-exit");
            return 1;
        }
    }
    if wait_status(P_PID, o as i32, WNOHANG) != Err(Errno::ECHILD) || pid_info(o as i32).is_ok() {
        println!("proc-orphan: fail second-reaper");
        return 1;
    }
    if let Ok(info) = pid_info(g as i32)
        && info[1] != 1
    {
        println!("proc-orphan: fail grandchild-reparent");
        return 1;
    }
    write64(slot(SLOT_G_EXIT), 3);
    match wait_status(P_PID, g as i32, 0) {
        Ok((pid, status)) if pid == g && exit_code(status) == 3 => {}
        _ => {
            println!("proc-orphan: fail grandchild-exit");
            return 1;
        }
    }

    let p2 = match fork_thread(MODE_BATCH_PARENT, 0) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-orphan: fail fork-batch");
            return 1;
        }
    };
    let mut batch = [0usize; BATCH_COUNT];
    for index in 0..BATCH_COUNT {
        batch[index] = match wait_slot(BATCH_BASE + index) {
            Some(pid) => pid as usize,
            None => {
                println!("proc-orphan: fail batch-pid");
                return 1;
            }
        };
    }
    match wait_status(P_PID, p2 as i32, 0) {
        Ok((pid, status)) if pid == p2 && exit_code(status) == 5 => {}
        _ => {
            println!("proc-orphan: fail batch-parent-exit");
            return 1;
        }
    }
    for pid in batch {
        if let Ok(info) = pid_info(pid as i32)
            && info[1] != 1
        {
            println!("proc-orphan: fail batch-reparent");
            return 1;
        }
    }
    write64(slot(SLOT_BATCH_EXIT), 42);
    for pid in batch {
        match wait_status(P_PID, pid as i32, 0) {
            Ok((waited, status)) if waited == pid && exit_code(status) == 42 => {}
            _ => {
                println!("proc-orphan: fail batch-reap");
                return 1;
            }
        }
        if pid_info(pid as i32).is_ok() {
            println!("proc-orphan: fail batch-zombie");
            return 1;
        }
    }

    if !probe() {
        println!("proc-orphan: fail final-probe");
        return 1;
    }
    println!("proc-orphan: reparent=1 parent-exit=5");
    println!("proc-orphan: nonchild=echild");
    println!("proc-orphan: orphan-exit=7 reaped=1");
    println!("proc-orphan: grandchild-exit=3 reparent=1");
    println!("proc-orphan: batch=10 reaped=10");
    println!("proc-orphan: ok");
    0
}
