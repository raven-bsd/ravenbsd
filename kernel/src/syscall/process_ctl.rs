// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `PROC_CREATE` and `PROC_WAIT`.

use alloc::sync::Arc;
use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;
#[cfg(target_arch = "x86_64")]
use raven_abi::types::namespace;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

#[cfg(target_arch = "x86_64")]
use crate::ipc::model::{self, NAME_MAX};
#[cfg(target_arch = "x86_64")]
use crate::ipc::router;
use crate::process::{self, ProcessState, handle::Object};
use crate::sched;
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_to_user;

/// `proc_create(flags) -> Handle`.
pub fn create(frame: &SyscallFrame) -> Result<usize, Errno> {
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

/// Authorized termination (`PROC_TERMINATE`, proposed ABI number 0x32).
///
/// `proc_terminate(proc: Handle, code: i32) -> 0`.  The caller must hold a process handle with
/// the `SIGNAL` right; an unprivileged caller gets `EPERM`, a terminal target `ESRCH`.
pub fn terminate(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let code = frame.rsi as i32;
    let target = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::SIGNAL) {
            Ok(Object::Process(target)) => Arc::clone(target),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    if Arc::ptr_eq(&target, &caller) {
        // C16: terminating self behaves like exit(code).
        process::exit_process(code);
    }
    process::terminate(&target, code)?;
    Ok(0)
}

/// `PROC_NAMESPACE`: add, remove or clear scheme namespace prefixes.
///
/// Contract C17: `proc` is `SELF_PROC` or a process handle with `ADMIN`; a prefix may only be
/// added inside the caller's own namespace. Registration enforcement is staged for the follow-up
/// ticket (svcd must first grant service prefixes), so only `OPEN` consumes the namespace.
#[cfg(target_arch = "x86_64")]
pub fn namespace(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let op = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let target = if handle == Handle::SELF_PROC {
        Arc::clone(&caller)
    } else {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::ADMIN) {
            Ok(Object::Process(target)) => Arc::clone(target),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    let prefix = match op {
        namespace::ADD | namespace::REMOVE => {
            let prefix = router::user_string(frame.rdx, frame.r10, NAME_MAX)?;
            if !model::valid_scheme_name(&prefix) {
                return Err(Errno::EINVAL);
            }
            prefix
        }
        namespace::CLEAR if frame.r10 == 0 => alloc::string::String::new(),
        namespace::CLEAR => return Err(Errno::EINVAL),
        _ => return Err(Errno::EINVAL),
    };
    if op == namespace::ADD && !caller.namespace.lock().can_grant_prefix(&prefix) {
        return Err(Errno::EPERM);
    }
    let mut target_namespace = target.namespace.lock();
    match op {
        namespace::ADD => target_namespace.allow_prefix(&prefix),
        namespace::REMOVE => target_namespace.remove_prefix(&prefix),
        namespace::CLEAR => target_namespace.clear_prefixes(),
        _ => return Err(Errno::EINVAL),
    }
    Ok(0)
}

/// Scheme IPC is x86-64-only; the namespace syscall reports `ENOSYS` until the port.
#[cfg(not(target_arch = "x86_64"))]
pub fn namespace(_frame: &SyscallFrame) -> Result<usize, Errno> {
    Err(Errno::ENOSYS)
}

/// `proc_wait(proc, out_code) -> 0`.
pub fn wait(frame: &SyscallFrame) -> Result<usize, Errno> {
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
        if *state == ProcessState::Created {
            // A created-but-never-started process has no thread and can never run: wait reaps
            // it immediately (the terminal state and the one-shot teardown are the same).
            drop(state);
            let _ = process::reap_unstarted(&target);
            continue;
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
