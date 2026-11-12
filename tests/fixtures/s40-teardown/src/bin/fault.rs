// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S40 faulting client: a blocked scheme request and two waiters, then a deliberate user page
//! fault. The kernel must tear the process down without a double fault.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // The deliberate fault write; the block has a SAFETY comment.

use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::RtSys;
use raven_rt::sys::{nanosleep, scheme_register, write};
use raven_rt::thread;
use raven_s40_teardown::{FAULT_ADDRESS, FAULT_SCHEME, HOLD_PATH, open_retry};

entry!(main);

/// Stack for each helper thread.
const HELPER_STACK: usize = 16 * 1024;
/// Let the helpers reach their blocking syscalls before the fault.
const SETTLE_NS: u64 = 20_000_000;

/// Block in a scheme WRITE whose server holds the reply.
fn blocked_writer(arg: usize) {
    let _ = write(Handle(arg as u32), b"held");
}

/// Block in PROC_WAIT on this process's own handle (the process wait queue).
fn self_waiter(arg: usize) {
    let mut code = 0i32;
    let _ = RtSys.proc_wait(Handle(arg as u32), &mut code);
}

fn main(_args: raven_rt::Args) -> i32 {
    // Registration proves the teardown's registry cleanup for a dying process.
    if scheme_register(FAULT_SCHEME).is_err() {
        println!("s40: fault register failed");
        return 1;
    }
    let file = match open_retry(HOLD_PATH) {
        Ok(file) => file,
        Err(_) => {
            println!("s40: fault open failed");
            return 1;
        }
    };
    let _writer = match thread::spawn(blocked_writer, file.0 as usize, HELPER_STACK) {
        Ok(thread) => thread,
        Err(_) => {
            println!("s40: fault writer spawn failed");
            return 1;
        }
    };
    let _waiter = match thread::spawn(self_waiter, Handle::SELF_PROC.0 as usize, HELPER_STACK) {
        Ok(thread) => thread,
        Err(_) => {
            println!("s40: fault waiter spawn failed");
            return 1;
        }
    };
    let _ = nanosleep(SETTLE_NS);
    // SAFETY: the address is deliberately unmapped; the kernel's user page-fault path must
    // terminate this process (-11) without taking the machine down.
    unsafe {
        core::ptr::write_volatile(FAULT_ADDRESS as *mut u8, 0x41);
    }
    println!("s40: fault not taken");
    1
}
