// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `PROC_CREATE` and `PROC_WAIT`.

use alloc::sync::Arc;
use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

use crate::arch::x86_64::trapframe::TrapFrame;
use crate::process::{self, ProcessState, handle::Object};
use crate::sched;
use crate::usercopy::copy_to_user;

/// `proc_create(flags) -> Handle`.
pub fn create(frame: &TrapFrame) -> Result<usize, Errno> {
    if frame.rdi != 0 {
        return Err(Errno::EINVAL);
    }
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let child = process::new_empty_process()?;
    let raw = caller
        .handles
        .lock()
        .insert(Object::Process(child), Rights::ALL)?;
    Ok(raw.0 as usize)
}

/// `proc_wait(proc, out_code) -> 0`.
pub fn wait(frame: &TrapFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let target = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ)? {
            Object::Process(target) => Arc::clone(target),
            _ => return Err(Errno::EBADF),
        }
    };
    if Arc::ptr_eq(&target, &caller) {
        return Err(Errno::EINVAL);
    }
    let destination = UserSlice::new(frame.rsi, 4)?;

    loop {
        let state = target.state.lock();
        if *state == ProcessState::Exited {
            let code = target.exit_code.load(Ordering::Acquire);
            drop(state);
            copy_to_user(destination, &code.to_le_bytes())?;
            return Ok(0);
        }
        let token = sched::begin_block()?;
        // The state lock is held, so the process cannot pass Exited between the check above and
        // the enqueue; `mark_exited` takes the same lock before waking.
        let queued = target.waiters.enqueue_if(target.id, 0, || Ok(true));
        drop(state);
        match queued {
            Ok(true) => {}
            Ok(false) => {
                sched::end_block(token);
                continue;
            }
            Err(error) => {
                sched::end_block(token);
                return Err(error);
            }
        }
        let _ = sched::block_current();
        sched::end_block(token);
    }
}
