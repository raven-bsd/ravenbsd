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

/// Raw `proc_fork(src: Handle, flags: u64) -> Handle` number (S50b).
///
/// The ABI contract crate is reviewer-owned; the number is exposed here until the next ABI
/// revision adds a `Syscall` variant.
pub const PROC_FORK: usize = 0x39;

/// `proc_fork(src, flags) -> Handle`.
///
/// Clones `src`'s address space copy-on-write, duplicates its handle table with the same handle
/// numbers and shared lineage, copies the namespace and returns a handle to the child. The child
/// has no thread and does not run until the caller creates one with `THREAD_CREATE`. `0x34` is
/// `PROC_SET_PERSONALITY` and is not reused; every non-zero flag bit is `EINVAL` in v1.
pub fn fork(frame: &SyscallFrame) -> Result<usize, Errno> {
    if frame.rsi != 0 {
        return Err(Errno::EINVAL);
    }
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let source = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::ADMIN) {
            Ok(Object::Process(target)) => Arc::clone(target),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    if *source.state.lock() == ProcessState::Exited {
        return Err(Errno::ESRCH);
    }
    let child = process::new_forked_process(&source)?;
    let raw = caller
        .handles
        .lock()
        .insert(Object::Process(child), Rights::ALL)?;
    Ok(raw.0 as usize)
}

/// Raw `proc_self_info(out, count) -> 0` number (P2a).
pub const PROC_SELF_INFO: usize = 0x35;
/// Raw `proc_setpgid(pid, pgid) -> 0` number.
pub const PROC_SETPGID: usize = 0x36;
/// Raw `proc_setsid() -> sid` number.
pub const PROC_SETSID: usize = 0x37;
/// Raw `proc_setcred(which, value) -> 0` number.
pub const PROC_SETCRED: usize = 0x38;
/// Raw `proc_pid_info(pid, out, count) -> 0` number.
pub const PROC_PID_INFO: usize = 0x3A;

/// Write the eight identity words for `target` to user memory.
fn write_info(target: &Process, out: u64, count: usize) -> Result<usize, Errno> {
    if count != 8 {
        return Err(Errno::EINVAL);
    }
    let destination = UserSlice::new(out, 64)?;
    let ids = *target.ids.lock();
    let values = [
        target.id,
        ids.ppid,
        ids.pgid,
        ids.sid,
        u64::from(ids.uid),
        u64::from(ids.euid),
        u64::from(ids.gid),
        u64::from(ids.egid),
    ];
    let mut bytes = [0u8; 64];
    for (chunk, value) in bytes.chunks_exact_mut(8).zip(values) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    copy_to_user(destination, &bytes)?;
    Ok(0)
}

/// `proc_self_info(out: *mut u64, count: usize) -> 0`.
///
/// Writes `[pid, ppid, pgid, sid, uid, euid, gid, egid]` as eight little-endian u64 words.
/// Count must be exactly eight; the destination is checked by `UserSlice`.
pub fn self_info(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let count = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    write_info(&caller, frame.rdi, count)
}

/// `proc_pid_info(pid: i32, out: *mut u64, count: usize) -> 0`.
///
/// `pid == 0` means the caller. The target must still be in the pid registry and not exited.
pub fn pid_info(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let pid = frame.rdi as u32 as i32;
    let target = if pid == 0 {
        caller
    } else {
        let pid = u64::try_from(pid).map_err(|_| Errno::ESRCH)?;
        let target = process::lookup_pid(pid).ok_or(Errno::ESRCH)?;
        if *target.state.lock() == ProcessState::Exited {
            return Err(Errno::ESRCH);
        }
        target
    };
    let count = usize::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    write_info(&target, frame.rsi, count)
}

/// `proc_setpgid(pid: i32, pgid: i32) -> 0`.
///
/// The target is the caller, or a direct child. A zero pgid means the target's pid. A session
/// leader is `EPERM`. The target group may be the target's new own group, the current group, or
/// any group with a live member in the target's session. There is no group table; the pid
/// registry is scanned.
pub fn setpgid(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let pid = frame.rdi as u32 as i32;
    let target = if pid == 0 || u64::try_from(pid).ok() == Some(caller.id) {
        Arc::clone(&caller)
    } else {
        let pid = u64::try_from(pid).map_err(|_| Errno::ESRCH)?;
        let target = process::lookup_pid(pid).ok_or(Errno::ESRCH)?;
        if *target.state.lock() == ProcessState::Exited {
            return Err(Errno::ESRCH);
        }
        if target.ids.lock().ppid != caller.id {
            return Err(Errno::EPERM);
        }
        target
    };
    let target_id = target.id;
    let (target_sid, current_group) = {
        let ids = target.ids.lock();
        (ids.sid, ids.pgid)
    };
    if target_sid == target_id {
        return Err(Errno::EPERM);
    }
    let pgid = frame.rsi as u32 as i32;
    let group = if pgid == 0 {
        target_id
    } else {
        u64::try_from(pgid).map_err(|_| Errno::EPERM)?
    };
    let valid = group == target_id
        || group == current_group
        || process::group_in_session(group, target_sid);
    if !valid {
        return Err(Errno::EPERM);
    }
    target.ids.lock().pgid = group;
    Ok(0)
}

/// `proc_setsid() -> sid`.
///
/// A process-group leader cannot create a session. A successful call sets both the process group
/// and the session to the caller's pid.
pub fn setsid() -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let mut ids = caller.ids.lock();
    if ids.pgid == caller.id {
        return Err(Errno::EPERM);
    }
    ids.pgid = caller.id;
    ids.sid = caller.id;
    Ok(caller.id as usize)
}

/// `proc_setcred(which: u32, value: u32) -> 0`.
///
/// `which` 0..=3 selects uid/euid/gid/egid. Until a credential model exists the only permitted
/// change is to the current value; anything else is `EPERM`.
pub fn setcred(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let which = u32::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    let value = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let ids = caller.ids.lock();
    let current = match which {
        0 => ids.uid,
        1 => ids.euid,
        2 => ids.gid,
        3 => ids.egid,
        _ => return Err(Errno::EINVAL),
    };
    if value == current {
        Ok(0)
    } else {
        Err(Errno::EPERM)
    }
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
        let token = sched::begin_block_interruptible()?;
        if sched::take_kick() {
            sched::end_block(token);
            return Err(Errno::EINTR);
        }
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
        let status = sched::block_current();
        let kicked = sched::take_kick();
        sched::end_block(token);
        if status == Errno::EINTR.to_ret() || kicked {
            return Err(Errno::EINTR);
        }
    }
}
