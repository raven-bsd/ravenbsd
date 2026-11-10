// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! System call dispatch.

pub mod basic;
pub mod futex;
pub mod handle_xfer;
pub mod ipc;
pub mod memory_ctl;
pub mod process_ctl;
pub mod thread;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;

use crate::arch::x86_64::trapframe::TrapFrame;

/// Rust side of the `syscall` entry stub.
///
/// The assembly stub saves a TrapFrame-compatible register image, passes its address here, and
/// loads the return value from [`TrapFrame::rax`] after this function returns. `syscall_return`
/// is the last preemption point before `sysretq`.
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(frame: &mut TrapFrame) {
    let number = usize::try_from(frame.rax).unwrap_or(usize::MAX);
    let result = match Syscall::from_usize(number) {
        Some(Syscall::FUTEX) => futex::dispatch(frame),
        Some(Syscall::THREAD_CREATE) => thread::create(frame),
        Some(Syscall::THREAD_EXIT) => thread::exit(frame),
        Some(Syscall::PROC_CREATE) => process_ctl::create(frame),
        Some(Syscall::PROC_WAIT) => process_ctl::wait(frame),
        Some(Syscall::MEMOBJ_CREATE) => memory_ctl::memobj_create(frame),
        Some(Syscall::MAP) => memory_ctl::map(frame),
        Some(Syscall::UNMAP) => memory_ctl::unmap(frame),
        Some(Syscall::PROTECT) => memory_ctl::protect(frame),
        Some(Syscall::HANDLE_TRANSFER) => handle_xfer::transfer(frame),
        Some(
            call @ (Syscall::SCHEME_REGISTER
            | Syscall::OPEN
            | Syscall::READ
            | Syscall::WRITE
            | Syscall::CLOSE
            | Syscall::DUP
            | Syscall::FSTAT
            | Syscall::CALL
            | Syscall::RECV
            | Syscall::SEND),
        ) => ipc::dispatch(call, frame),
        Some(call) => basic::dispatch(call, frame),
        None => Err(Errno::ENOSYS),
    };
    let result = match result {
        Ok(value) => value as isize,
        Err(error) => error.to_ret(),
    };
    frame.rax = result as u64;
    crate::sched::syscall_return();
}
