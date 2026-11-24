// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K488 process-group/session contract fixture.

#![no_std]
#![no_main]

extern crate alloc;

#[path = "../common.rs"]
mod common;

use alloc::format;
use core::slice;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::exec::exec_image;
use raven_rt::pipe::pipe;
use raven_rt::println;
use raven_rt::sys;
use raven_rt::sys::{read, write};

use common::*;

entry!(main);

const CONTROL: usize = 0x4000_0000;
const SLOT_INITFS_BASE: usize = 20;
const SLOT_INITFS_LEN: usize = 21;
const SLOT_EXEC_FAIL: usize = 22;
const SLOT_PID: usize = 0;
const SLOT_PGID: usize = 1;
const SLOT_SID: usize = 2;
const SLOT_READY: usize = 3;
const SLOT_SETSID_AGAIN: usize = 4;

const MODE_LEADER: usize = 0;
const MODE_SETSID: usize = 1;
const MODE_EXEC: usize = 2;
const MODE_PLAIN: usize = 3;

fn slot(base: usize, field: usize) -> usize {
    CONTROL + (base + field) * 8
}

fn wait_slot(base: usize, field: usize) -> Option<u64> {
    for _ in 0..5000 {
        let value = read64(slot(base, field));
        if value != 0 {
            return Some(value);
        }
        sleep_ms();
    }
    None
}

extern "C" fn child_entry(arg: usize) -> ! {
    let mode = arg & 0xff;
    let base = arg >> 8;
    if mode == MODE_LEADER {
        let _ = setpgid(0, 0);
    } else if mode == MODE_SETSID {
        let _ = setsid();
        let again = setsid().is_err();
        write64(slot(base, SLOT_SETSID_AGAIN), u64::from(again));
    }
    if let Ok(info) = self_info() {
        write64(slot(base, SLOT_PID), info[0]);
        write64(slot(base, SLOT_PGID), info[2]);
        write64(slot(base, SLOT_SID), info[3]);
        write64(slot(base, SLOT_READY), 1);
    }
    loop {
        sleep_ms();
    }
}

extern "C" fn exec_entry(arg: usize) -> ! {
    let fd = Handle(arg as u32);
    let base = read64(slot(0, SLOT_INITFS_BASE)) as usize;
    let len = read64(slot(0, SLOT_INITFS_LEN)) as usize;
    // SAFETY: the parent wrote the live initfs mapping base and length into the shared page; the
    // fork child has the same mapping.
    let image = unsafe { slice::from_raw_parts(base as *const u8, len) };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            let _ = write(fd, &[0xEE]);
            sys::thread_exit(1);
        }
    };
    let Some(child) = initfs.get("/exec-child") else {
        let _ = write(fd, &[0xEE]);
        sys::thread_exit(1);
    };
    let fd_text = format!("{}", fd.0);
    let args: [&[u8]; 2] = [b"/exec-child", fd_text.as_bytes()];
    let _ = exec_image(child.data, b"/exec-child", &args, &[], &[]);
    let _ = write(fd, &[0xEE]);
    write64(slot(0, SLOT_EXEC_FAIL), 1);
    sys::thread_exit(1)
}

fn fork_child(mode: usize, base: usize, arg: usize) -> Result<usize, Errno> {
    let child = proc_fork(Handle::SELF_PROC)?;
    let entry = if mode == MODE_EXEC {
        exec_entry as *const () as usize
    } else {
        child_entry as *const () as usize
    };
    let thread_arg = if mode == MODE_EXEC {
        arg
    } else {
        (base << 8) | mode
    };
    let pid = handle_info(child)?;
    thread_create(child, entry, stack_top(), thread_arg)?;
    Ok(pid)
}

fn main(_args: raven_rt::Args) -> i32 {
    if control_page().is_none() {
        println!("proc-pgrp: fail control");
        return 1;
    }
    let initfs_image = match raven_rt::map_initfs() {
        Ok(image) => image,
        Err(_) => {
            println!("proc-pgrp: fail initfs");
            return 1;
        }
    };
    write64(
        slot(0, SLOT_INITFS_BASE),
        initfs_image.as_ptr() as usize as u64,
    );
    write64(slot(0, SLOT_INITFS_LEN), initfs_image.len() as u64);

    let parent = match self_info() {
        Ok(info) => info,
        Err(_) => {
            println!("proc-pgrp: fail self");
            return 1;
        }
    };
    let parent_pid = parent[0];
    let parent_pgid = parent[2];
    let parent_sid = parent[3];
    let getpgrp_ok = parent_pgid == parent_pid && getpgid(0) == Ok(parent_pid as usize);

    let a = match fork_child(MODE_LEADER, 0, 0) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-pgrp: fail fork-a");
            return 1;
        }
    };
    if wait_slot(0, SLOT_READY) != Some(1) {
        println!("proc-pgrp: fail ready-a");
        return 1;
    }
    let a_pgid = read64(slot(0, SLOT_PGID)) as usize;
    if a_pgid != a || getpgid(a as i32) != Ok(a) {
        println!("proc-pgrp: fail self-group");
        return 1;
    }
    if setpgid(0, -1) != Err(Errno::EINVAL) {
        println!("proc-pgrp: fail negative-pgid");
        return 1;
    }

    let a2 = match fork_child(MODE_PLAIN, 1, 0) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-pgrp: fail fork-a2");
            return 1;
        }
    };
    if wait_slot(1, SLOT_READY) != Some(1) || setpgid(a2 as i32, a as i32).is_err() {
        println!("proc-pgrp: fail join-a");
        return 1;
    }
    if getpgid(a2 as i32) != Ok(a) {
        println!("proc-pgrp: fail a2-group");
        return 1;
    }

    let b = match fork_child(MODE_PLAIN, 2, 0) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-pgrp: fail fork-b");
            return 1;
        }
    };
    if wait_slot(2, SLOT_READY) != Some(1)
        || read64(slot(2, SLOT_PGID)) as usize != parent_pgid as usize
    {
        println!("proc-pgrp: fail inherit");
        return 1;
    }
    if setpgid(b as i32, 0).is_err() || getpgid(b as i32) != Ok(b) {
        println!("proc-pgrp: fail setpgid-child");
        return 1;
    }

    let (exec_read, exec_write) = match pipe() {
        Ok(pair) => pair,
        Err(_) => {
            println!("proc-pgrp: fail pipe");
            return 1;
        }
    };
    let c = match fork_child(MODE_EXEC, 0, exec_write.0 as usize) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-pgrp: fail fork-exec");
            return 1;
        }
    };
    let mut byte = [0u8; 1];
    let exec_ok = read(exec_read, &mut byte) == Ok(1) && byte[0] == 1;
    if !exec_ok || setpgid(c as i32, 0) != Err(Errno::EACCES) {
        println!("proc-pgrp: fail after-exec");
        return 1;
    }

    let d = match fork_child(MODE_SETSID, 3, 0) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-pgrp: fail fork-setsid");
            return 1;
        }
    };
    if wait_slot(3, SLOT_READY) != Some(1)
        || read64(slot(3, SLOT_SID)) as usize != d
        || read64(slot(3, SLOT_SETSID_AGAIN)) != 1
    {
        println!("proc-pgrp: fail getsid");
        return 1;
    }
    if setpgid(d as i32, 0) != Err(Errno::EPERM) || setpgid(d as i32, b as i32) != Err(Errno::EPERM)
    {
        println!("proc-pgrp: fail cross-session");
        return 1;
    }

    if signal(-(a as i32), 0).is_err()
        || signal(-(b as i32), 0).is_err()
        || signal(-(9999i32), 0) != Err(Errno::ESRCH)
    {
        println!("proc-pgrp: fail group-kill");
        return 1;
    }

    if signal(a as i32, SIGKILL).is_err() {
        println!("proc-pgrp: fail kill-a");
        return 1;
    }
    let mut status = 0i32;
    if wait_ex(P_PGID, a as i32, 0, &mut status) != Ok(a) {
        println!("proc-pgrp: fail wait-a");
        return 1;
    }
    if getpgid(a2 as i32) != Ok(a) || signal(-(a as i32), 0).is_err() {
        println!("proc-pgrp: fail leader-lifetime");
        return 1;
    }
    let mut status2 = 0i32;
    if wait_ex(P_PGID, a as i32, WNOHANG, &mut status2) != Ok(0) {
        println!("proc-pgrp: fail wait-live-member");
        return 1;
    }
    let e = match fork_child(MODE_PLAIN, 4, 0) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-pgrp: fail fork-e");
            return 1;
        }
    };
    if e == a || getpgid(e as i32) != Ok(parent_pgid as usize) {
        println!("proc-pgrp: fail group-reuse");
        return 1;
    }
    if signal(a2 as i32, SIGKILL).is_err()
        || signal(b as i32, SIGKILL).is_err()
        || signal(c as i32, SIGKILL).is_err()
        || signal(d as i32, SIGKILL).is_err()
        || signal(e as i32, SIGKILL).is_err()
    {
        println!("proc-pgrp: fail cleanup");
        return 1;
    }
    if wait_ex(P_PGID, a as i32, 0, &mut status) != Ok(a2) {
        println!("proc-pgrp: fail wait-a2-group");
        return 1;
    }
    if getpgid(b as i32) != Ok(b) {
        println!("proc-pgrp: fail wait-b-other-group");
        return 1;
    }
    let _ = wait_ex(P_PID, b as i32, 0, &mut status);
    let _ = wait_ex(P_PID, d as i32, 0, &mut status);
    let _ = wait_ex(P_PID, e as i32, 0, &mut status);
    let _ = wait_ex(P_PID, c as i32, 0, &mut status);

    if !getpgrp_ok || parent_sid != parent_pid {
        println!("proc-pgrp: fail parent-identity");
        return 1;
    }
    println!("proc-pgrp: getpgrp=1 inherit=1");
    println!("proc-pgrp: setpgid-self=1 setpgid-child=1 negative-pgid=1");
    println!("proc-pgrp: after-exec=eacces");
    println!("proc-pgrp: cross-session=eperm group-eperm=1");
    println!("proc-pgrp: getsid=1 setsid-again=eperm");
    println!("proc-pgrp: group-kill=1 group-wait=1");
    println!("proc-pgrp: leader-lifetime=1 group-reuse=1");
    println!("proc-pgrp: ok");
    0
}
