// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2f signal fixture: SIGUSR queue delivery and SIGCHLD on child exit.

#![no_std]
#![no_main]

#[path = "../common.rs"]
mod common;

use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;

use common::*;

entry!(main);

const CONTROL: usize = 0x4000_0000;
const SLOT_READY: usize = 0;
const SLOT_RX1: usize = 1;
const SLOT_RX2: usize = 2;
const SLOT_ERROR: usize = 3;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_signal_child_entry
    .type proc_signal_child_entry, @function
proc_signal_child_entry:
    call proc_signal_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size proc_signal_child_entry, . - proc_signal_child_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_signal_child_entry
    .type proc_signal_child_entry, %function
proc_signal_child_entry:
    bl proc_signal_child_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size proc_signal_child_entry, . - proc_signal_child_entry
"#
);

unsafe extern "C" {
    fn proc_signal_child_entry();
}

/// Child thread: wait for two queued signals, publish and exit.
#[unsafe(no_mangle)]
pub extern "C" fn proc_signal_child_body() -> i32 {
    write64(CONTROL + SLOT_READY * 8, 1);
    let mut first = [0u64; 3];
    let Ok(signo) = sigrecv(&mut first, 0) else {
        write64(CONTROL + SLOT_ERROR * 8, 1);
        return 1;
    };
    write64(CONTROL + SLOT_RX1 * 8, signo as u64);
    let mut second = [0u64; 3];
    let Ok(signo) = sigrecv(&mut second, 0) else {
        write64(CONTROL + SLOT_ERROR * 8, 2);
        return 2;
    };
    write64(CONTROL + SLOT_RX2 * 8, signo as u64);
    0
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

fn main(_args: raven_rt::Args) -> i32 {
    let Some(control) = control_page() else {
        println!("proc-signal: fail control");
        return 1;
    };
    let _ = control;
    let child = match proc_fork(Handle::SELF_PROC) {
        Ok(child) => child,
        Err(_) => {
            println!("proc-signal: fail fork");
            return 1;
        }
    };
    let pid = match handle_info(child) {
        Ok(pid) => pid,
        Err(_) => {
            println!("proc-signal: fail pid");
            return 1;
        }
    };
    let thread = match thread_create(
        child,
        proc_signal_child_entry as *const () as usize,
        stack_top(),
        0,
    ) {
        Ok(thread) => thread,
        Err(_) => {
            println!("proc-signal: fail thread");
            return 1;
        }
    };
    if wait_slot(SLOT_READY) != Some(1) {
        println!("proc-signal: fail ready");
        return 1;
    }
    if signal(pid as i32, SIGUSR1).is_err() {
        println!("proc-signal: fail send1");
        return 1;
    }
    if wait_slot(SLOT_RX1) != Some(SIGUSR1 as u64) {
        println!("proc-signal: fail rx1");
        return 1;
    }
    if signal(pid as i32, SIGUSR2).is_err() {
        println!("proc-signal: fail send2");
        return 1;
    }
    if wait_slot(SLOT_RX2) != Some(SIGUSR2 as u64) {
        println!("proc-signal: fail rx2");
        return 1;
    }
    let mut status = 0i32;
    match wait_ex(P_PID, pid as i32, 0, &mut status) {
        Ok(waited) if waited == pid && status == 0 => {}
        _ => {
            println!("proc-signal: fail wait");
            return 1;
        }
    }
    let mut info = [0u64; 3];
    let chld = sigrecv(&mut info, WNOHANG);
    let mut empty = [0u64; 3];
    let eagain = sigrecv(&mut empty, WNOHANG) == Err(raven_abi::errno::Errno::EAGAIN);
    let invalid = signal(pid as i32, 0) == Err(raven_abi::errno::Errno::EINVAL);
    let unknown = signal(9999, SIGUSR1) == Err(raven_abi::errno::Errno::ESRCH);
    println!("proc-signal: rx1={SIGUSR1} rx2={SIGUSR2}");
    println!("proc-signal: wait pid={pid} status=0");
    if let Ok(signo) = chld {
        println!("proc-signal: sigchld={signo} sender={}", info[1]);
    } else {
        println!("proc-signal: fail sigchld");
        return 1;
    }
    println!(
        "proc-signal: eagain={} invalid={} unknown={}",
        u8::from(eagain),
        u8::from(invalid),
        u8::from(unknown)
    );
    let _ = close(thread);
    let _ = close(child);
    if !eagain || !invalid || !unknown {
        println!("proc-signal: fail checks");
        return 1;
    }
    println!("proc-signal: ok");
    0
}
