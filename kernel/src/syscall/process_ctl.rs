// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `PROC_CREATE` and `PROC_WAIT`.

use alloc::sync::Arc;
use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;
use raven_abi::types::namespace;
use raven_abi::types::personality::CREATE_LINUX;
use raven_abi::types::{Handle, Personality, Rights};
use raven_usercopy::UserSlice;

use crate::ipc::model::{self, NAME_MAX};
use crate::ipc::router;
use crate::process::{self, Process, ProcessState, handle::Object};
use crate::sched;
use crate::syscall::SyscallFrame;
use crate::syscall::linux::install_handler;
use crate::usercopy::copy_to_user;

/// `proc_create(flags) -> Handle`.
pub fn create(frame: &SyscallFrame) -> Result<usize, Errno> {
    let flags = usize::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    if flags & !CREATE_LINUX != 0 {
        return Err(Errno::EINVAL);
    }
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let child = process::new_empty_process()?;
    if flags & CREATE_LINUX != 0 {
        child.personality.lock().personality = Personality::Linux;
    }
    let raw = caller
        .handles
        .lock()
        .insert(Object::Process(child), Rights::ALL)?;
    Ok(raw.0 as usize)
}

/// `proc_set_personality(proc, personality, handler) -> 0`.
///
/// The child must not have started a thread unless it is the caller itself;
/// the handler is duplicated from the caller's table and stored in the target.
pub fn set_personality(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let raw_personality = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let personality =
        Personality::from_u8(u8::try_from(raw_personality).map_err(|_| Errno::EINVAL)?)
            .ok_or(Errno::EINVAL)?;
    let handler = Handle(u32::try_from(frame.rdx).map_err(|_| Errno::EBADF)?);

    let target: Arc<Process> = if handle == Handle::SELF_PROC {
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
    if !Arc::ptr_eq(&target, &caller) && target.threads.load(Ordering::Acquire) != 0 {
        return Err(Errno::EINVAL);
    }
    if *target.state.lock() == ProcessState::Exited {
        return Err(Errno::ESRCH);
    }

    let new_handler = install_handler(&caller, &target, handler)?;

    let old = {
        let mut state = target.personality.lock();
        let old = state.handler;
        state.personality = personality;
        state.handler = new_handler;
        old
    };
    if let Some(old) = old
        && new_handler != Some(old)
    {
        let _ = crate::event::close_handle(&target, old);
    }
    Ok(0)
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
