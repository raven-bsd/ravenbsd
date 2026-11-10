// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `THREAD_CREATE` and `THREAD_EXIT`.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::is_user_address;

use crate::arch::x86_64::trapframe::TrapFrame;
use crate::process::{self, handle::Object};
use crate::sched;

/// `thread_create(proc, entry, stack_top, arg, flags) -> Thread handle`.
///
/// Only the calling process is accepted in S09. The entry and stack top must be user addresses;
/// the stack top must be 16-byte aligned; all flag bits are reserved and must be zero.
pub fn create(frame: &TrapFrame) -> Result<usize, Errno> {
    let process = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    {
        let handles = process.handles.lock();
        match handles.get(handle, Rights::WRITE)? {
            Object::Process(target) if Arc::ptr_eq(target, &process) => {}
            _ => return Err(Errno::EACCES),
        }
    }

    if frame.r8 != 0 {
        return Err(Errno::EINVAL);
    }
    let entry = frame.rsi;
    let stack_top = frame.rdx;
    if !is_user_address(entry) {
        return Err(Errno::EFAULT);
    }
    if stack_top & 0xf != 0 || !is_user_address(stack_top.saturating_sub(1)) {
        return Err(Errno::EFAULT);
    }

    let thread = sched::new_user_thread(&process, entry, stack_top, frame.r10)?;
    let rights = Rights::SIGNAL.union(Rights::DUP);
    let raw = process
        .handles
        .lock()
        .insert(Object::Thread(Arc::clone(&thread)), rights)?;
    if !sched::enqueue(Arc::clone(&thread)) {
        let _ = process.handles.lock().close(raw);
        return Err(Errno::EAGAIN);
    }
    Ok(raw.0 as usize)
}

/// `thread_exit(code) -> !`.
pub fn exit(frame: &TrapFrame) -> ! {
    sched::exit_thread(frame.rdi as i32)
}
