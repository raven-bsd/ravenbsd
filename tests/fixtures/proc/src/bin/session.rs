// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2f session fixture: groups, sessions, stop/continue, wait id types, limits and times.

#![no_std]
#![no_main]

#[path = "../common.rs"]
mod common;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;

use common::*;

entry!(main);

const CONTROL: usize = 0x4000_0000;
const SLOT_PID_A: usize = 0;
const SLOT_PID_B: usize = 1;
const SLOT_READY_A: usize = 2;
const SLOT_READY_B: usize = 3;
const SLOT_SID_B: usize = 4;
const SLOT_PGID_B: usize = 5;
const SLOT_MODE: usize = 6;

const STOP_STATUS: i32 = (19 << 8) | 0x7f;
const CONT_STATUS: i32 = 0xffff;
const KILL_STATUS: i32 = (128 + 9) << 8;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_session_child_entry
    .type proc_session_child_entry, @function
proc_session_child_entry:
    call proc_session_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size proc_session_child_entry, . - proc_session_child_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_session_child_entry
    .type proc_session_child_entry, %function
proc_session_child_entry:
    bl proc_session_child_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size proc_session_child_entry, . - proc_session_child_entry
"#
);

unsafe extern "C" {
    fn proc_session_child_entry();
}

/// Child body: publish identity, optionally create a session, then sleep until killed.
#[unsafe(no_mangle)]
pub extern "C" fn proc_session_child_body() -> i32 {
    let mode = read64(CONTROL + SLOT_MODE * 8);
    if mode == 1 {
        if setsid().is_err() {
            return 1;
        }
    }
    let Ok(info) = self_info() else {
        return 2;
    };
    if mode == 1 {
        write64(CONTROL + SLOT_PID_B * 8, info[0]);
        write64(CONTROL + SLOT_PGID_B * 8, info[2]);
        write64(CONTROL + SLOT_SID_B * 8, info[3]);
        write64(CONTROL + SLOT_READY_B * 8, 1);
    } else {
        write64(CONTROL + SLOT_PID_A * 8, info[0]);
        write64(CONTROL + SLOT_READY_A * 8, 1);
    }
    loop {
        sleep_ms();
    }
}

fn wait_slot(slot: usize) -> Option<u64> {
    for _ in 0..5000 {
        let value = read64(CONTROL + slot * 8);
        if value != 0 {
            return Some(value);
        }
        sleep_ms();
    }
    None
}

fn wait_status(idtype: usize, id: i32, options: usize) -> Result<(usize, i32), Errno> {
    let mut status = 0i32;
    wait_ex(idtype, id, options, &mut status).map(|pid| (pid, status))
}

fn main(_args: raven_rt::Args) -> i32 {
    let Some(control) = control_page() else {
        println!("proc-session: fail control");
        return 1;
    };
    let _ = control;

    write64(CONTROL + SLOT_MODE * 8, 0);
    let child_a = match proc_fork(Handle::SELF_PROC) {
        Ok(child) => child,
        Err(_) => {
            println!("proc-session: fail fork-a");
            return 1;
        }
    };
    if thread_create(
        child_a,
        proc_session_child_entry as *const () as usize,
        stack_top(),
        0,
    )
    .is_err()
    {
        println!("proc-session: fail thread-a");
        return 1;
    }
    let Some(pid_a) = wait_slot(SLOT_PID_A) else {
        println!("proc-session: fail pid-a");
        return 1;
    };
    if wait_slot(SLOT_READY_A) != Some(1) {
        println!("proc-session: fail ready-a");
        return 1;
    }

    write64(CONTROL + SLOT_MODE * 8, 1);
    let child_b = match proc_fork(Handle::SELF_PROC) {
        Ok(child) => child,
        Err(_) => {
            println!("proc-session: fail fork-b");
            return 1;
        }
    };
    if thread_create(
        child_b,
        proc_session_child_entry as *const () as usize,
        stack_top(),
        0,
    )
    .is_err()
    {
        println!("proc-session: fail thread-b");
        return 1;
    }
    let Some(pid_b) = wait_slot(SLOT_PID_B) else {
        println!("proc-session: fail pid-b");
        return 1;
    };
    if wait_slot(SLOT_READY_B) != Some(1) {
        println!("proc-session: fail ready-b");
        return 1;
    }

    let parent_pgid = getpgid(0);
    let a_pgid = getpgid(pid_a as i32);
    if parent_pgid != Ok(1) || a_pgid != Ok(1) {
        println!("proc-session: fail initial groups");
        return 1;
    }
    if setpgid(pid_a as i32, pid_a as i32).is_err() || getpgid(pid_a as i32) != Ok(pid_a as usize) {
        println!("proc-session: fail setpgid");
        return 1;
    }
    if getpgid(pid_b as i32) != Ok(pid_b as usize)
        || read64(CONTROL + SLOT_PGID_B * 8) != pid_b
        || read64(CONTROL + SLOT_SID_B * 8) != pid_b
    {
        println!("proc-session: fail session");
        return 1;
    }

    if signal(-(pid_a as i32), SIGSTOP).is_err() {
        println!("proc-session: fail stop");
        return 1;
    }
    let (stopped_pid, stop_status) = match wait_status(P_PID, pid_a as i32, WUNTRACED) {
        Ok(value) => value,
        Err(_) => {
            println!("proc-session: fail wait-stop");
            return 1;
        }
    };
    if stopped_pid != pid_a as usize || stop_status != STOP_STATUS {
        println!("proc-session: fail stop-status");
        return 1;
    }
    if signal(pid_a as i32, SIGCONT).is_err() {
        println!("proc-session: fail cont");
        return 1;
    }
    let (cont_pid, cont_status) = match wait_status(P_PID, pid_a as i32, WCONTINUED) {
        Ok(value) => value,
        Err(_) => {
            println!("proc-session: fail wait-cont");
            return 1;
        }
    };
    if cont_pid != pid_a as usize || cont_status != CONT_STATUS {
        println!("proc-session: fail cont-status");
        return 1;
    }
    if wait_status(P_ALL, 0, WNOHANG) != Ok((0, 0)) {
        println!("proc-session: fail nohang");
        return 1;
    }

    let times_ok = times().is_ok();
    let rusage_ok = getrusage().is_ok();
    let default_nproc = getrlimit(RLIMIT_NPROC);
    if default_nproc.is_err() || default_nproc.as_ref().is_ok_and(|limit| limit[0] == 0) {
        println!("proc-session: fail rlimit-default");
        return 1;
    }
    let set_nproc = setrlimit(RLIMIT_NPROC, 1, 1);
    let nproc = getrlimit(RLIMIT_NPROC);
    let invalid_cur_max = setrlimit(RLIMIT_NPROC, 2, 1) == Err(Errno::EINVAL);
    let invalid_resource = getrlimit(99) == Err(Errno::EINVAL);
    if set_nproc.is_err() || nproc != Ok([1, 1]) || !invalid_cur_max || !invalid_resource {
        println!("proc-session: fail rlimit");
        return 1;
    }

    if signal(pid_a as i32, SIGKILL).is_err() {
        println!("proc-session: fail kill-a");
        return 1;
    }
    let (kill_a_pid, kill_a_status) = match wait_status(P_PGID, pid_a as i32, 0) {
        Ok(value) => value,
        Err(_) => {
            println!("proc-session: fail wait-kill-a");
            return 1;
        }
    };
    if kill_a_pid != pid_a as usize || kill_a_status != KILL_STATUS {
        println!("proc-session: fail kill-a-status");
        return 1;
    }
    if signal(pid_b as i32, SIGKILL).is_err() {
        println!("proc-session: fail kill-b");
        return 1;
    }
    let (kill_b_pid, kill_b_status) = match wait_status(P_ALL, 0, 0) {
        Ok(value) => value,
        Err(_) => {
            println!("proc-session: fail wait-kill-b");
            return 1;
        }
    };
    if kill_b_pid != pid_b as usize || kill_b_status != KILL_STATUS {
        println!("proc-session: fail kill-b-status");
        return 1;
    }

    let echild = wait_status(P_PID, 9999, WNOHANG) == Err(Errno::ECHILD);
    let unknown_pgid = getpgid(9999) == Err(Errno::ESRCH);
    let set_zero = setrlimit(RLIMIT_NPROC, 0, 0);
    let nproc_zero = proc_fork(Handle::SELF_PROC) == Err(Errno::EAGAIN);
    let _ = setrlimit(RLIMIT_NPROC, 1024, 1024);
    if set_zero.is_err() || !nproc_zero || !echild || !unknown_pgid {
        println!("proc-session: fail final");
        return 1;
    }

    println!("proc-session: pgid=1 child-a={pid_a} child-b={pid_b}");
    println!("proc-session: setpgid=1 getpgid=1 sid-b=1");
    println!("proc-session: stop={stop_status} cont={cont_status}");
    println!("proc-session: kill-a={kill_a_status} kill-b={kill_b_status}");
    println!(
        "proc-session: times={} rusage={} rlimit=1 invalid=1 nproc=1 echild=1",
        u8::from(times_ok),
        u8::from(rusage_ok)
    );
    println!("proc-session: ok");
    0
}
