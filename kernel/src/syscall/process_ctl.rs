// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `PROC_CREATE` and `PROC_WAIT`.

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;
use raven_abi::types::namespace;
use raven_abi::types::personality::CREATE_LINUX;
use raven_abi::types::{Handle, Personality, Rights};
use raven_usercopy::UserSlice;

use crate::ipc::model::{self, NAME_MAX, PATH_MAX};
use crate::ipc::{registry, router};
use crate::process::status::{
    DefaultAction, P_ALL, P_PGID, P_PID, RLIM_INFINITY, RLIMIT_COUNT, RLIMIT_NPROC, Rlimit,
    SIGKILL, WAIT_OPTIONS, WCONTINUED, WNOHANG, WUNTRACED, WaitEvent, default_action, signal_valid,
};
use crate::process::{self, Process, ProcessState, handle::Object};
use crate::sched;
use crate::syscall::SyscallFrame;
use crate::syscall::linux::install_handler;
use crate::usercopy::{copy_from_user, copy_to_user};

/// `proc_create(flags) -> Handle`.
/// `proc_reboot(how)` powers off, halts or resets the machine. Root-only.
pub fn reboot(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    if caller.ids.lock().euid != 0 {
        return Err(Errno::EPERM);
    }
    let how = u32::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    if how & !raven_abi::syscall::RB_FLAGS != 0 {
        return Err(Errno::EINVAL);
    }
    if how & raven_abi::syscall::RB_POWER_OFF != 0 {
        crate::kprintln!("power: poweroff");
        crate::arch::poweroff()
    } else if how & raven_abi::syscall::RB_HALT != 0 {
        crate::kprintln!("power: halt");
        crate::arch::halt()
    } else {
        crate::kprintln!("power: reboot");
        crate::arch::reset()
    }
}

pub fn create(frame: &SyscallFrame) -> Result<usize, Errno> {
    let flags = usize::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    if flags & !CREATE_LINUX != 0 {
        return Err(Errno::EINVAL);
    }
    let caller = process::current().ok_or(Errno::ESRCH)?;
    if !can_create_child(&caller) {
        return Err(Errno::EAGAIN);
    }
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

/// K66 wire read bit.
const WIRE_OPEN_READ: u32 = 1;
/// K66 wire require-directory bit.
const WIRE_OPEN_DIRECTORY: u32 = 0x40;

/// `proc_chroot(path: *const u8, len: usize) -> 0`.
///
/// The caller must be root (`euid == 0`). The path is resolved inside the current root, opened
/// with `O_DIRECTORY` so a missing path or a non-directory is the server's exact errno, and then
/// stored as the normalized kernel-visible root. Fork, `PROC_CREATE` and `PROC_EXEC` inherit it;
/// the next `chroot` resolves against it.
pub fn chroot(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    if caller.ids.lock().euid != 0 {
        return Err(Errno::EPERM);
    }
    let path = router::user_string(frame.rdi, frame.rsi, PATH_MAX)?;
    let (scheme, suffix) = split_root_path(&path);
    let root = caller.root.lock().clone();
    let target = process::resolve_root_path(&root, suffix);
    let server = registry::lookup(&caller, scheme)?;
    let context = (root != "/").then_some(root.as_str());
    let handle = router::open(
        &caller,
        server,
        &target,
        context,
        WIRE_OPEN_READ | WIRE_OPEN_DIRECTORY,
    )?;
    router::close(&caller, handle)?;
    *caller.root.lock() = target;
    Ok(0)
}

/// Split an optional `file:`/`vfs:` scheme from a chroot path; a bare path uses `file:`.
fn split_root_path(path: &str) -> (&str, &str) {
    if let Some((scheme, rest)) = path.split_once(':')
        && matches!(scheme, "file" | "vfs")
    {
        return (scheme, rest);
    }
    ("file", path)
}

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
    if !can_create_child(&caller) {
        return Err(Errno::EAGAIN);
    }
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
/// Raw `proc_set_command(proc, name, len) -> 0` number (P2o).
pub const PROC_SET_COMMAND: usize = 0x45;
/// Maximum `PROC_PID_INFO` words: eight identity words, a 256-byte name, and twenty K391 words.
pub const PROC_INFO_MAX_WORDS: usize = 61;
/// Raw `proc_setcred_ex(which, a, b, c, groups, count) -> 0` number.
pub const PROC_SETCRED_EX: usize = 0x3B;
/// Raw `proc_cred_info(out, count) -> 0` number.
pub const PROC_CRED_INFO: usize = 0x3C;
/// Raw `proc_getgroups(out, size) -> count` number.
pub const PROC_GETGROUPS: usize = 0x3D;
/// Raw `proc_handle_info(handle) -> pid` number.
pub const PROC_HANDLE_INFO: usize = 0x3E;
/// Raw `proc_exec_cred(proc, file, flags) -> 0` number.
pub const PROC_EXEC_CRED: usize = 0x3F;
/// Raw `proc_wait_ex(idtype, id, options, out_status) -> pid` number.
pub const PROC_WAIT_EX: usize = 0x48;
/// Raw `proc_getpgid(pid) -> pgid` number.
pub const PROC_GETPGID: usize = 0x49;
/// Raw `proc_signal(target, signo) -> 0` number.
pub const PROC_SIGNAL: usize = 0x4A;
/// Raw `proc_sigrecv(out, flags) -> signo` number (three-word legacy output).
pub const PROC_SIGRECV: usize = 0x4B;
/// Raw `proc_sigrecv_ex(out, flags, deliverable) -> signo` number (six-word output).
pub const PROC_SIGRECV_EX: usize = 0x85;
/// Raw `proc_sigqueue(target, signo, value) -> 0` number.
pub const PROC_SIGQUEUE: usize = 0x86;
/// `PROC_SIGRECV_EX` flag: select only signals whose bit is in the `deliverable` mask.
const SIGRECV_DELIVERABLE: u32 = 2;
/// Raw `proc_times(out, count) -> 0` number.
pub const PROC_TIMES: usize = 0x4C;
/// Raw `proc_getrusage(who, out, count) -> 0` number.
pub const PROC_GETRUSAGE: usize = 0x4D;
/// Raw `proc_getrlimit(resource, out) -> 0` number.
pub const PROC_GETRLIMIT: usize = 0x4E;
/// Raw `proc_setrlimit(resource, cur, max) -> 0` number.
pub const PROC_SETRLIMIT: usize = 0x4F;
/// Suppress setuid/setgid bits for this exec.
pub const EXEC_NOSUID: u64 = 1;

const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;
const S_ISUID: u32 = 0o4000;
const S_ISGID: u32 = 0o2000;

/// Encode the identity and optional command name for `target`.
///
/// The first eight words are unchanged from P2a. When `count > 8`, the UTF-8 command name is
/// written NUL-terminated at byte offset 64, truncated to the remaining buffer at a character
/// boundary.
fn encode_info(target: &Process, count: usize) -> Result<Vec<u8>, Errno> {
    if !(8..=PROC_INFO_MAX_WORDS).contains(&count) {
        return Err(Errno::EINVAL);
    }
    let length = count.checked_mul(8).ok_or(Errno::EINVAL)?;
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
    let mut bytes = vec![0u8; length];
    for (chunk, value) in bytes
        .get_mut(..64)
        .ok_or(Errno::EINVAL)?
        .chunks_exact_mut(8)
        .zip(values)
    {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    if count > 8 {
        let command = target.command.lock();
        // The name area stays at byte 64; the K391 diagnostic words start at byte 328.
        let capacity = length.saturating_sub(64).saturating_sub(1).min(256);
        let mut end = 0usize;
        for (index, character) in command.char_indices() {
            let next = index.saturating_add(character.len_utf8());
            if next > capacity {
                break;
            }
            end = next;
        }
        if let (Some(slot), Some(source)) = (
            bytes.get_mut(64..64usize.saturating_add(end)),
            command.as_bytes().get(..end),
        ) {
            slot.copy_from_slice(source);
        }
    }
    if count >= PROC_INFO_MAX_WORDS {
        let state = match *target.state.lock() {
            ProcessState::Created => 0u64,
            ProcessState::Running => 1u64,
            ProcessState::Exited => 2u64,
        };
        let runnable = u64::from(
            target.threads.load(Ordering::Acquire) > 0 && !target.exiting.load(Ordering::Acquire),
        );
        // Take one queue lock at a time: the diagnostic must not create a lock-order edge
        // between the client, delivery and server queues of a live process.
        let client_len = u64::try_from(target.client_requests.lock().len()).unwrap_or(u64::MAX);
        let queue_len = u64::try_from(target.scheme_queue.lock().len()).unwrap_or(u64::MAX);
        let active_len = u64::try_from(target.active_requests.lock().len()).unwrap_or(u64::MAX);
        let last_wake_cpu = target.wake_last_cpu.load(Ordering::Relaxed);
        let (cpu_need_resched, cpu_runqueue) = if last_wake_cpu == u32::MAX {
            (0u64, 0u64)
        } else {
            let cpu = usize::try_from(last_wake_cpu).unwrap_or(0);
            let (need, len) = crate::sched::cpu_status(cpu);
            (u64::from(need), len as u64)
        };
        let waiter_len = u64::try_from(target.scheme_waiters.len()).unwrap_or(u64::MAX);
        let waiter_key = target
            .scheme_waiters
            .first_key()
            .map_or(0, |(space, addr)| (space << 32) | (addr & 0xffff_ffff));
        let diagnostics = [
            state,
            runnable,
            target.last_syscall.load(Ordering::Relaxed),
            target.last_pc.load(Ordering::Relaxed),
            target.wait_object.load(Ordering::Relaxed),
            u64::from(target.in_syscall.load(Ordering::Relaxed)),
            target.last_submit_id.load(Ordering::Relaxed),
            target.last_recv_id.load(Ordering::Relaxed),
            target.last_send_id.load(Ordering::Relaxed),
            client_len,
            queue_len,
            active_len,
            waiter_len,
            waiter_key,
            u64::from(target.wake_fail.load(Ordering::Relaxed)),
            target.wake_fail_detail.load(Ordering::Relaxed),
            u64::from(target.wake_pending.load(Ordering::Relaxed)),
            u64::from(target.wake_last_cpu.load(Ordering::Relaxed)),
            cpu_need_resched,
            cpu_runqueue,
        ];
        for (chunk, value) in bytes
            .get_mut(328..488)
            .ok_or(Errno::EINVAL)?
            .chunks_exact_mut(8)
            .zip(diagnostics)
        {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
    }
    Ok(bytes)
}

/// Write the identity words (and optional name) for `target` to user memory.
fn write_info(target: &Process, out: u64, count: usize) -> Result<usize, Errno> {
    let bytes = encode_info(target, count)?;
    let length = u64::try_from(bytes.len()).map_err(|_| Errno::EINVAL)?;
    copy_to_user(UserSlice::new(out, length)?, &bytes)?;
    Ok(0)
}

/// `proc_self_info(out: *mut u64, count: usize) -> 0`.
///
/// Writes `[pid, ppid, pgid, sid, uid, euid, gid, egid]` as eight little-endian u64 words.
/// With `count` in `9..=41`, the command name follows at byte offset 64. The destination is
/// checked by `UserSlice`.
pub fn self_info(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let count = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    write_info(&caller, frame.rdi, count)
}

/// `proc_pid_info(pid: i32, out: *mut u64, count: usize) -> 0`.
///
/// `pid == 0` means the caller. The target must still be in the pid registry and not exited.
/// `count=8` preserves the P2a payload; `count` in `9..=41` appends the command name at byte
/// offset 64.
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

/// `proc_set_command(proc: Handle, name: &[u8]) -> 0` (raw 0x45, P2o).
///
/// `proc` is `SELF_PROC` or a process handle with `ADMIN`. The name is UTF-8 without NUL and is
/// truncated to [`process::MAX_COMMAND`] bytes at a character boundary.
pub fn set_command(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let length = usize::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    if length > process::MAX_COMMAND {
        return Err(Errno::ENAMETOOLONG);
    }
    let mut bytes = vec![0u8; length];
    if length != 0 {
        let slice = UserSlice::new(frame.rsi, u64::try_from(length).map_err(|_| Errno::EINVAL)?)?;
        copy_from_user(&mut bytes, slice)?;
    }
    if bytes.contains(&0) {
        return Err(Errno::EINVAL);
    }
    let text = core::str::from_utf8(&bytes).map_err(|_| Errno::EILSEQ)?;
    let target = if handle == Handle::SELF_PROC {
        caller
    } else {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::ADMIN) {
            Ok(Object::Process(target)) => Arc::clone(target),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    if *target.state.lock() == ProcessState::Exited {
        return Err(Errno::ESRCH);
    }
    *target.command.lock() = process::truncate_command(text);
    Ok(0)
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
    let pgid = frame.rsi as u32 as i32;
    if pgid < 0 {
        return Err(Errno::EINVAL);
    }
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
    if target_id != caller.id {
        // A child must still be in the caller's session and must not have exec'd. The donor
        // order is same-session EPERM before the exec EACCES.
        if target_sid != caller.ids.lock().sid {
            return Err(Errno::EPERM);
        }
        if target.exec_done.load(Ordering::Acquire) {
            return Err(Errno::EACCES);
        }
    }
    if target_sid == target_id {
        return Err(Errno::EPERM);
    }
    let group = if pgid == 0 {
        target_id
    } else {
        u64::try_from(pgid).map_err(|_| Errno::EINVAL)?
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
    drop(ids);
    crate::process::tty::clear_session(&caller);
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

/// `proc_cred_info(out: *mut u64, count: usize) -> 0`.
///
/// Writes `[uid, euid, suid, gid, egid, sgid]` as six little-endian u64 words. The caller only.
pub fn cred_info(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let count = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if count != 6 {
        return Err(Errno::EINVAL);
    }
    let destination = UserSlice::new(frame.rdi, 48)?;
    let ids = *caller.ids.lock();
    let values = [
        u64::from(ids.uid),
        u64::from(ids.euid),
        u64::from(ids.suid),
        u64::from(ids.gid),
        u64::from(ids.egid),
        u64::from(ids.sgid),
    ];
    let mut bytes = [0u8; 48];
    for (chunk, value) in bytes.chunks_exact_mut(8).zip(values) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    copy_to_user(destination, &bytes)?;
    Ok(0)
}

/// `proc_getgroups(out: *mut u32, size: usize) -> count`.
///
/// `size == 0` returns the count without touching user memory. A smaller buffer is `EINVAL`;
/// a larger buffer receives exactly `count` entries in kernel storage order.
pub fn getgroups(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let size = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let ids = caller.ids.lock();
    let count = usize::try_from(ids.group_count).map_err(|_| Errno::EINVAL)?;
    if size == 0 {
        return Ok(count);
    }
    if size < count || count > process::MAX_GROUPS {
        return Err(Errno::EINVAL);
    }
    let destination = UserSlice::new(
        frame.rdi,
        u64::try_from(count)
            .map_err(|_| Errno::EINVAL)?
            .saturating_mul(4),
    )?;
    let mut bytes = [0u8; 64];
    for (chunk, value) in bytes.chunks_exact_mut(4).zip(ids.groups.iter()) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    copy_to_user(
        destination,
        bytes.get(..count.saturating_mul(4)).ok_or(Errno::EINVAL)?,
    )?;
    Ok(count)
}

fn read_groups(pointer: u64, count: usize) -> Result<([u32; process::MAX_GROUPS], usize), Errno> {
    if count > process::MAX_GROUPS {
        return Err(Errno::EINVAL);
    }
    let mut groups = [0u32; process::MAX_GROUPS];
    if count == 0 {
        return Ok((groups, 0));
    }
    let mut bytes = [0u8; 64];
    copy_from_user(
        bytes
            .get_mut(..count.saturating_mul(4))
            .ok_or(Errno::EINVAL)?,
        UserSlice::new(
            pointer,
            u64::try_from(count)
                .map_err(|_| Errno::EINVAL)?
                .saturating_mul(4),
        )?,
    )?;
    for (chunk, value) in bytes
        .chunks_exact(4)
        .take(count)
        .zip(groups.iter_mut().take(count))
    {
        let mut word = [0u8; 4];
        word.copy_from_slice(chunk);
        *value = u32::from_le_bytes(word);
    }
    Ok((groups, count))
}

fn one(value: i32) -> Result<Option<u32>, Errno> {
    if value < -1 {
        return Err(Errno::EINVAL);
    }
    Ok(if value == -1 {
        None
    } else {
        Some(value as u32)
    })
}

fn unprivileged(value: Option<u32>, choices: &[u32]) -> Result<(), Errno> {
    match value {
        Some(value) if !choices.contains(&value) => Err(Errno::EPERM),
        _ => Ok(()),
    }
}

/// `proc_setcred_ex(which, a, b, c, groups, count) -> 0`.
///
/// `which` is the `SetCred` kind: 1 setuid, 2 seteuid, 3 setreuid, 4 setresuid, 5 setgid,
/// 6 setegid, 7 setregid, 8 setresgid, 9 setgroups. Effective uid 0 is privileged; an
/// unprivileged process may only target its real, effective or saved id and may never set
/// supplementary groups.
pub fn setcred_ex(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let which = u32::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    if !(1..=9).contains(&which) {
        return Err(Errno::EINVAL);
    }
    let a = frame.rsi as u32 as i32;
    let b = frame.rdx as u32 as i32;
    let c = frame.r10 as u32 as i32;
    let count = usize::try_from(frame.r9).map_err(|_| Errno::EINVAL)?;
    let (groups, group_count) = if which == 9 {
        read_groups(frame.r8, count)?
    } else {
        ([0u32; process::MAX_GROUPS], 0)
    };
    let mut ids = caller.ids.lock();
    let privileged = ids.euid == 0;
    match which {
        1 | 5 => {
            let Some(value) = one(a)? else {
                return Err(Errno::EINVAL);
            };
            if which == 1 {
                if privileged {
                    ids.uid = value;
                    ids.euid = value;
                    ids.suid = value;
                } else {
                    unprivileged(Some(value), &[ids.uid, ids.suid])?;
                    ids.euid = value;
                }
            } else if privileged {
                ids.gid = value;
                ids.egid = value;
                ids.sgid = value;
            } else {
                unprivileged(Some(value), &[ids.gid, ids.sgid])?;
                ids.egid = value;
            }
        }
        2 | 6 => {
            let Some(value) = one(a)? else {
                return Err(Errno::EINVAL);
            };
            if which == 2 {
                if !privileged {
                    unprivileged(Some(value), &[ids.uid, ids.suid])?;
                }
                ids.euid = value;
            } else {
                if !privileged {
                    unprivileged(Some(value), &[ids.gid, ids.sgid])?;
                }
                ids.egid = value;
            }
        }
        3 | 7 => {
            let real = one(a)?;
            let effective = one(b)?;
            if privileged {
                if let Some(real) = real {
                    if which == 3 {
                        ids.uid = real;
                    } else {
                        ids.gid = real;
                    }
                }
                if let Some(effective) = effective {
                    if which == 3 {
                        ids.euid = effective;
                        ids.suid = effective;
                    } else {
                        ids.egid = effective;
                        ids.sgid = effective;
                    }
                }
            } else if which == 3 {
                unprivileged(real, &[ids.uid])?;
                unprivileged(effective, &[ids.uid, ids.euid, ids.suid])?;
                if let Some(real) = real {
                    ids.uid = real;
                }
                if let Some(effective) = effective {
                    ids.euid = effective;
                }
            } else {
                unprivileged(real, &[ids.gid])?;
                unprivileged(effective, &[ids.gid, ids.egid, ids.sgid])?;
                if let Some(real) = real {
                    ids.gid = real;
                }
                if let Some(effective) = effective {
                    ids.egid = effective;
                }
            }
        }
        4 | 8 => {
            let real = one(a)?;
            let effective = one(b)?;
            let saved = one(c)?;
            if !privileged {
                let (current_real, current_effective, current_saved) = if which == 4 {
                    (ids.uid, ids.euid, ids.suid)
                } else {
                    (ids.gid, ids.egid, ids.sgid)
                };
                unprivileged(real, &[current_real, current_effective, current_saved])?;
                unprivileged(effective, &[current_real, current_effective, current_saved])?;
                unprivileged(saved, &[current_real, current_effective, current_saved])?;
            }
            if let Some(real) = real {
                if which == 4 {
                    ids.uid = real;
                } else {
                    ids.gid = real;
                }
            }
            if let Some(effective) = effective {
                if which == 4 {
                    ids.euid = effective;
                } else {
                    ids.egid = effective;
                }
            }
            if let Some(saved) = saved {
                if which == 4 {
                    ids.suid = saved;
                } else {
                    ids.sgid = saved;
                }
            }
        }
        9 => {
            if !privileged {
                return Err(Errno::EPERM);
            }
            ids.groups = groups;
            ids.group_count = u32::try_from(group_count).map_err(|_| Errno::EINVAL)?;
        }
        _ => return Err(Errno::EINVAL),
    }
    Ok(0)
}

/// `proc_handle_info(handle) -> pid`.
///
/// The handle must already be in the caller's table and name a process; the syscall does not
/// confer rights or search ambient tables. A non-process or stale handle is `EBADF`. The result
/// is the kernel-assigned pid, including for a child that has already exited.
pub fn handle_info(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let target = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ) {
            Ok(Object::Process(target)) => Arc::clone(target),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    usize::try_from(target.id).map_err(|_| Errno::EINVAL)
}

/// `proc_exec_cred(proc, file, flags) -> 0`.
///
/// The caller has already parsed the executable image. This syscall takes the open scheme-file
/// handle, obtains its `Stat` through the kernel's inline-FSTAT path, checks the caller's execute
/// permission, then applies a regular file's setuid/setgid bits to the target child's effective
/// and saved ids. `flags & EXEC_NOSUID` suppresses both bits. The target must be an empty child
/// (no threads) with a process handle the caller may administer; applying credentials to the
/// caller itself is refused.
pub fn exec_cred(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file_handle = Handle(u32::try_from(frame.rsi).map_err(|_| Errno::EBADF)?);
    let flags = frame.rdx;
    if flags & !EXEC_NOSUID != 0 {
        return Err(Errno::EINVAL);
    }
    if handle == Handle::SELF_PROC {
        return Err(Errno::EPERM);
    }
    let target = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::ADMIN) {
            Ok(Object::Process(target)) => Arc::clone(target),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    if Arc::ptr_eq(&target, &caller) {
        return Err(Errno::EPERM);
    }
    if target.threads.load(Ordering::Acquire) != 0 {
        return Err(Errno::EINVAL);
    }
    if *target.state.lock() == ProcessState::Exited {
        return Err(Errno::ESRCH);
    }

    let file = crate::ipc::file_from_handle(&caller, file_handle, Rights::READ)?;
    let bytes = router::fstat_kernel(&caller, &file)?;
    let mode = u32::from_le_bytes([bytes[48], bytes[49], bytes[50], bytes[51]]);
    let uid = u32::from_le_bytes([bytes[56], bytes[57], bytes[58], bytes[59]]);
    let gid = u32::from_le_bytes([bytes[60], bytes[61], bytes[62], bytes[63]]);
    if mode & S_IFMT != S_IFREG {
        return Err(Errno::EACCES);
    }
    // A scheme server's `fstat` is not authority for root credentials.  Elevated bits are
    // accepted only from the kernel-recorded live owner of a boot-trusted executable source.
    if mode & (S_ISUID | S_ISGID) != 0 && !registry::file_source_trusted(&file.server) {
        return Err(Errno::EPERM);
    }

    let caller_ids = caller.ids.lock();
    if !executable(&caller_ids, mode, uid, gid) {
        return Err(Errno::EACCES);
    }
    drop(caller_ids);

    if flags & EXEC_NOSUID != 0 {
        return Ok(0);
    }
    let mut ids = target.ids.lock();
    if mode & S_ISUID != 0 {
        ids.euid = uid;
        ids.suid = uid;
    }
    if mode & S_ISGID != 0 {
        ids.egid = gid;
        ids.sgid = gid;
    }
    Ok(0)
}

fn executable(ids: &process::ProcessIds, mode: u32, uid: u32, gid: u32) -> bool {
    if ids.euid == 0 {
        return mode & 0o111 != 0;
    }
    let bits = if ids.euid == uid {
        (mode >> 6) & 0o7
    } else if ids.egid == gid
        || ids
            .groups
            .iter()
            .take(ids.group_count as usize)
            .any(|group| *group == gid)
    {
        (mode >> 3) & 0o7
    } else {
        mode & 0o7
    };
    bits & 0o1 != 0
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
        crate::syscall::record_lock::release_process(caller.id);
        process::exit_process(code);
    }
    crate::syscall::record_lock::release_process(target.id);
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

    let mut interrupted = false;
    loop {
        let state = target.state.lock();
        if *state == ProcessState::Exited {
            let code = target.exit_code.load(Ordering::Acquire);
            drop(state);
            copy_to_user(destination, &code.to_le_bytes())?;
            let _ = process::reap_pid(&target);
            return Ok(0);
        }
        if *state == ProcessState::Created {
            // A created-but-never-started process has no thread and can never run: wait reaps
            // it immediately (the terminal state and the one-shot teardown are the same).
            drop(state);
            let _ = process::reap_unstarted(&target);
            let _ = process::reap_pid(&target);
            continue;
        }
        // The child exit queues SIGCHLD and kicks this thread; the target state may already be
        // terminal on the wakeup. Re-check it before reporting the interruption as `EINTR`.
        if interrupted {
            drop(state);
            return Err(Errno::EINTR);
        }
        let token = sched::begin_block_interruptible()?;
        if sched::take_kick() {
            sched::end_block(token);
            drop(state);
            interrupted = true;
            continue;
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
        interrupted = status == Errno::EINTR.to_ret() || kicked;
    }
}

/// True when `caller` is under its `RLIMIT_NPROC` live-process limit.
fn can_create_child(caller: &Arc<Process>) -> bool {
    let limit = caller
        .limits
        .lock()
        .get(RLIMIT_NPROC)
        .copied()
        .unwrap_or(Rlimit::new(RLIM_INFINITY, RLIM_INFINITY))
        .cur;
    if limit == RLIM_INFINITY {
        return true;
    }
    let uid = caller.ids.lock().uid;
    let count = process::processes()
        .iter()
        .filter(|process| {
            process.ids.lock().uid == uid && *process.state.lock() != ProcessState::Exited
        })
        .count();
    (count as u64) < limit
}

/// Snapshot the children selected by a wait id type.
fn children_for(caller: &Arc<Process>, idtype: u32, id: i32) -> Result<Vec<Arc<Process>>, Errno> {
    match idtype {
        P_PID => {
            let pid = u64::try_from(id).map_err(|_| Errno::ECHILD)?;
            let child = process::lookup_pid(pid).ok_or(Errno::ECHILD)?;
            if child.ids.lock().ppid != caller.id {
                return Err(Errno::ECHILD);
            }
            Ok(vec![child])
        }
        P_PGID => {
            let group = if id == 0 {
                caller.ids.lock().pgid
            } else {
                u64::try_from(id).map_err(|_| Errno::EINVAL)?
            };
            Ok(process::processes()
                .into_iter()
                .filter(|child| {
                    let ids = *child.ids.lock();
                    ids.ppid == caller.id && ids.pgid == group
                })
                .collect())
        }
        P_ALL => Ok(process::processes()
            .into_iter()
            .filter(|child| child.ids.lock().ppid == caller.id)
            .collect()),
        _ => Err(Errno::EINVAL),
    }
}

/// Non-destructive child-event probe for one child.
fn peek_event(child: &Process, options: u32) -> Option<WaitEvent> {
    if *child.state.lock() == ProcessState::Exited && !child.reaped.load(Ordering::Acquire) {
        return Some(WaitEvent::Exited(child.exit_code.load(Ordering::Acquire)));
    }
    if options & WUNTRACED != 0 {
        let signo = child.stop_signal.load(Ordering::Acquire);
        if signo != 0 && !child.stop_reported.load(Ordering::Acquire) {
            return Some(WaitEvent::Stopped(signo));
        }
    }
    if options & WCONTINUED != 0 && child.continued.load(Ordering::Acquire) {
        return Some(WaitEvent::Continued);
    }
    None
}

/// Consume the flags associated with a selected event.
fn take_event(child: &Process, event: WaitEvent) {
    match event {
        WaitEvent::Exited(_) => {
            let _ = process::reap_pid(child);
        }
        WaitEvent::Stopped(_) => child.stop_reported.store(true, Ordering::Release),
        WaitEvent::Continued => child.continued.store(false, Ordering::Release),
    }
}

/// Consume one ready child event, if any.
///
/// The caller's selection is re-checked at every pass: a child exit may race a signal kick
/// (`SIGCHLD`), and the status must win over `EINTR` when both are ready. This mirrors the
/// FreeBSD `kern_wait6` loop, which re-scans the child list after `msleep` before returning the
/// interrupted error.
fn take_ready_event(
    caller: &Arc<Process>,
    idtype: u32,
    id: i32,
    options: u32,
    destination: UserSlice,
) -> Result<Option<usize>, Errno> {
    let children = children_for(caller, idtype, id)?;
    if children.is_empty() {
        return Err(Errno::ECHILD);
    }
    for child in &children {
        if let Some(event) = peek_event(child, options) {
            let status = event.status();
            take_event(child, event);
            let pid = usize::try_from(child.id).map_err(|_| Errno::EINVAL)?;
            copy_to_user(destination, &status.to_le_bytes())?;
            return Ok(Some(pid));
        }
    }
    Ok(None)
}

/// `proc_wait_ex(idtype, id, options, out_status) -> pid`.
///
/// `P_PID`, `P_PGID` and `P_ALL` select children; `WNOHANG` returns 0 when no
/// event is ready. Exit, stop and continue statuses use the POSIX encoding from
/// [`crate::process::status`]. A blocking wait sleeps on the caller's child queue
/// and is woken by exit/stop/continue notifications. A caught signal that
/// interrupts the sleep returns `EINTR` (FreeBSD `kern_wait6` `msleep(... PCATCH)`,
/// NetBSD `find_stopped_child`); the caller retries and the status stays queued.
pub fn wait_ex(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let idtype = u32::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    let id = frame.rsi as u32 as i32;
    let options = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    if options & !WAIT_OPTIONS != 0 {
        return Err(Errno::EINVAL);
    }
    let destination = UserSlice::new(frame.r10, 4)?;

    loop {
        if let Some(pid) = take_ready_event(&caller, idtype, id, options, destination)? {
            return Ok(pid);
        }
        if options & WNOHANG != 0 {
            return Ok(0);
        }

        let token = sched::begin_block_interruptible()?;
        if sched::take_kick() {
            sched::end_block(token);
            return Err(Errno::EINTR);
        }
        let queued = caller.child_waiters.enqueue_if(caller.id, 0, || {
            let children = children_for(&caller, idtype, id)?;
            if children.is_empty() {
                return Ok(false);
            }
            Ok(!children
                .iter()
                .any(|child| peek_event(child, options).is_some()))
        });
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
            // A child-exit notification is a plain wake, but its `SIGCHLD` queue entry kicks
            // this thread through the PG-7 bridge. If the status is already ready, return it;
            // only a signal with no child event is an interruption.
            if let Some(pid) = take_ready_event(&caller, idtype, id, options, destination)? {
                return Ok(pid);
            }
            return Err(Errno::EINTR);
        }
    }
}

/// `proc_getpgid(pid) -> pgid`. Zero means the caller.
pub fn getpgid(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let pid = frame.rdi as u32 as i32;
    let target = if pid == 0 {
        caller
    } else {
        let pid = u64::try_from(pid).map_err(|_| Errno::ESRCH)?;
        process::lookup_pid(pid).ok_or(Errno::ESRCH)?
    };
    usize::try_from(target.ids.lock().pgid).map_err(|_| Errno::EINVAL)
}

/// True when `caller` may signal `target`.
fn signal_allowed(caller: &Process, target: &Process) -> bool {
    let caller_ids = *caller.ids.lock();
    if caller_ids.euid == 0 {
        return true;
    }
    let target_ids = *target.ids.lock();
    caller_ids.euid == target_ids.uid
        || caller_ids.uid == target_ids.uid
        || caller_ids.euid == target_ids.euid
        || caller_ids.euid == target_ids.suid
}

fn signal_one(caller: &Arc<Process>, target: &Arc<Process>, signo: u32) -> Result<(), Errno> {
    signal_one_info(caller, target, signo, process::SI_USER_CODE, 0, 0)
}

/// Deliver one signal with an explicit queued record (`sigqueue`).
fn signal_one_info(
    caller: &Arc<Process>,
    target: &Arc<Process>,
    signo: u32,
    code: u32,
    uid: u32,
    value: u64,
) -> Result<(), Errno> {
    if signo == SIGKILL {
        return terminate_by_signal(caller, target, SIGKILL, false);
    }
    if !target.has_signal_entry()
        && let Some(action) = default_action(signo)
    {
        match action {
            DefaultAction::Terminate => {
                return terminate_by_signal(caller, target, signo, false);
            }
            DefaultAction::Core => {
                return terminate_by_signal(caller, target, signo, true);
            }
            DefaultAction::Ignore => return Ok(()),
            // Stop/continue keep the existing job-control bookkeeping in `deliver_signal_info`.
            DefaultAction::Stop | DefaultAction::Continue => {}
        }
    }
    process::deliver_signal_info(target, signo, caller.id, code, uid, value)
}

/// Apply the default terminate action for a process without a registered libc entry.
///
/// The exit code is negative so `WaitEvent::status` reports the POSIX signal status (low seven
/// bits, plus the core bit) rather than a normal exit byte.
fn terminate_by_signal(
    caller: &Arc<Process>,
    target: &Arc<Process>,
    signo: u32,
    core: bool,
) -> Result<(), Errno> {
    let raw = (signo & 0x7f) | if core { 0x80 } else { 0 };
    let code = -(raw as i32);
    if Arc::ptr_eq(caller, target) {
        process::exit_process(code);
    }
    process::terminate(target, code)
}

/// Resolve a `PROC_SIGNAL` target into the concrete processes it names.
///
/// Positive `target` is a pid, zero is the caller's process group and a negative value is
/// `-pgid`. The caller must be allowed to signal every resolved process.
fn resolve_targets(caller: &Arc<Process>, target: i32) -> Result<Vec<Arc<Process>>, Errno> {
    let targets: Vec<Arc<Process>> = if target > 0 {
        let pid = u64::try_from(target).map_err(|_| Errno::ESRCH)?;
        let target = process::lookup_pid(pid).ok_or(Errno::ESRCH)?;
        if *target.state.lock() == ProcessState::Exited {
            return Err(Errno::ESRCH);
        }
        vec![target]
    } else {
        let group = if target == 0 {
            caller.ids.lock().pgid
        } else {
            u64::from(target.unsigned_abs())
        };
        process::processes()
            .into_iter()
            .filter(|process| {
                process.ids.lock().pgid == group && *process.state.lock() != ProcessState::Exited
            })
            .collect()
    };
    if targets.is_empty() {
        return Err(Errno::ESRCH);
    }
    for target in &targets {
        if !signal_allowed(caller, target) {
            return Err(Errno::EPERM);
        }
    }
    Ok(targets)
}

/// `proc_signal(target, signo) -> 0`.
///
/// Positive `target` is a pid, zero is the caller's process group and a negative
/// value is `-pgid`. `SIGKILL` terminates, stop signals set the wait-status stop
/// state, `SIGCONT` clears it and every other signal is queued for
/// `PROC_SIGRECV`. The permission check uses the caller's effective/real uid and
/// the target's real/saved uid; effective uid 0 is privileged.
pub fn signal(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let target = frame.rdi as u32 as i32;
    let signo = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if signo != 0 && !signal_valid(signo) {
        return Err(Errno::EINVAL);
    }
    let targets = resolve_targets(&caller, target)?;
    if signo == 0 {
        // POSIX `kill(pid, 0)` is an existence/permission check; it never queues a signal.
        return Ok(0);
    }
    for target in targets {
        signal_one(&caller, &target, signo)?;
    }
    Ok(0)
}

/// `proc_sigqueue(target, signo, value) -> 0`.
///
/// Same target and permission contract as `PROC_SIGNAL`, but the queued record carries the
/// caller's real uid, `SI_QUEUE` and the 64-bit `sigval` payload. The kernel does not interpret
/// the payload; `PROC_SIGRECV_EX` returns it to libc.
pub fn sigqueue(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let target = frame.rdi as u32 as i32;
    let signo = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let value = frame.rdx;
    if signo == 0 || !signal_valid(signo) {
        return Err(Errno::EINVAL);
    }
    let targets = resolve_targets(&caller, target)?;
    let uid = caller.ids.lock().uid;
    for target in targets {
        signal_one_info(&caller, &target, signo, process::SI_QUEUE_CODE, uid, value)?;
    }
    Ok(0)
}

/// `proc_sigrecv(out, flags) -> signo`.
///
/// Writes `[signo, sender_pid, 0]` as three little-endian u64 words. `WNOHANG`
/// returns `EAGAIN` instead of blocking. The kernel queue does not interpret
/// signal dispositions; libc owns the user pending set.
pub fn sigrecv(frame: &SyscallFrame) -> Result<usize, Errno> {
    receive(frame, 3, false)
}

/// `proc_sigrecv_ex(out, flags, deliverable) -> signo`.
///
/// Writes six little-endian u64 words `[signo, sender_pid, si_code, si_uid, si_value, 0]`.
/// `flags` bit 0 is `WNOHANG` and bit 1 (`DELIVERABLE`) makes the kernel select the oldest signal
/// whose bit is in the `deliverable` mask, leaving every other signal queued. That is the masked
/// queue contract: a blocked signal is not consumed (Illumos `fsig`, FreeBSD `sigtd`, NetBSD
/// `sigismasked`).
pub fn sigrecv_ex(frame: &SyscallFrame) -> Result<usize, Errno> {
    receive(frame, 6, true)
}

/// The one-bit mask of a signal number (the libc `APP_MASK` layout).
fn signal_bit(signo: u32) -> u64 {
    1u64.wrapping_shl(signo.saturating_sub(1) & 63)
}

/// Shared receive path for the legacy three-word and the extended six-word call.
fn receive(frame: &SyscallFrame, words: usize, extended: bool) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let destination = UserSlice::new(frame.rdi, (words * 8) as u64)?;
    let flags = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let allowed = if extended {
        WNOHANG | SIGRECV_DELIVERABLE
    } else {
        WNOHANG
    };
    if flags & !allowed != 0 {
        return Err(Errno::EINVAL);
    }
    let selects = extended && flags & SIGRECV_DELIVERABLE != 0;
    let deliverable = if selects { frame.rdx } else { 0 };
    loop {
        let selected = if selects {
            caller.pop_signal_where(|signal| deliverable & signal_bit(signal.signo) != 0)
        } else {
            caller.pop_signal()
        };
        if let Some(signal) = selected {
            let values = if extended {
                [
                    u64::from(signal.signo),
                    signal.sender,
                    u64::from(signal.code),
                    u64::from(signal.uid),
                    signal.value,
                    0,
                ]
            } else {
                [u64::from(signal.signo), signal.sender, 0, 0, 0, 0]
            };
            let mut bytes = [0u8; 48];
            for (chunk, value) in bytes.chunks_exact_mut(8).zip(values).take(words) {
                chunk.copy_from_slice(&value.to_le_bytes());
            }
            copy_to_user(destination, &bytes[..words * 8])?;
            return Ok(signal.signo as usize);
        }
        if flags & WNOHANG != 0 {
            return Err(Errno::EAGAIN);
        }
        let token = sched::begin_block_interruptible()?;
        if sched::take_kick() {
            sched::end_block(token);
            return Err(Errno::EINTR);
        }
        let queued = caller.signal_waiters.enqueue_if(caller.id, 0, || {
            let empty = if selects {
                !caller
                    .has_pending_signal_where(|signal| deliverable & signal_bit(signal.signo) != 0)
            } else {
                !caller.has_pending_signal()
            };
            Ok(empty)
        });
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

/// `proc_times(out, count) -> 0`; count must be four u64 words.
pub fn times(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let count = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if count != 4 {
        return Err(Errno::EINVAL);
    }
    let destination = UserSlice::new(frame.rdi, 32)?;
    let values = [
        caller.user_ns.load(Ordering::Acquire),
        caller.system_ns.load(Ordering::Acquire),
        caller.child_user_ns.load(Ordering::Acquire),
        caller.child_system_ns.load(Ordering::Acquire),
    ];
    let mut bytes = [0u8; 32];
    for (chunk, value) in bytes.chunks_exact_mut(8).zip(values) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    copy_to_user(destination, &bytes)?;
    Ok(0)
}

/// `proc_getrusage(who, out, count) -> 0`.
///
/// `who == 0` is the caller, `who == -1` the reaped children. Eight u64 words:
/// user ns, system ns, maxrss, minflt, majflt, nvcsw, nivcsw, reserved.
pub fn getrusage(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let who = frame.rdi as u32 as i32;
    let count = usize::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    if count != 8 {
        return Err(Errno::EINVAL);
    }
    let (user, system, maxrss) = match who {
        0 => {
            caller.update_maxrss();
            (
                caller.user_ns.load(Ordering::Acquire),
                caller.system_ns.load(Ordering::Acquire),
                caller.maxrss_bytes.load(Ordering::Acquire),
            )
        }
        -1 => (
            caller.child_user_ns.load(Ordering::Acquire),
            caller.child_system_ns.load(Ordering::Acquire),
            caller.child_maxrss_bytes.load(Ordering::Acquire),
        ),
        _ => return Err(Errno::EINVAL),
    };
    let destination = UserSlice::new(frame.rsi, 64)?;
    let values = [user, system, maxrss, 0, 0, 0, 0, caller.start_ns];
    let mut bytes = [0u8; 64];
    for (chunk, value) in bytes.chunks_exact_mut(8).zip(values) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    copy_to_user(destination, &bytes)?;
    Ok(0)
}

/// `proc_getrlimit(resource, out) -> 0`; writes `[cur, max]`.
pub fn getrlimit(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let resource = usize::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    if resource >= RLIMIT_COUNT {
        return Err(Errno::EINVAL);
    }
    let destination = UserSlice::new(frame.rsi, 16)?;
    let limit = caller
        .limits
        .lock()
        .get(resource)
        .copied()
        .ok_or(Errno::EINVAL)?;
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&limit.cur.to_le_bytes());
    bytes[8..].copy_from_slice(&limit.max.to_le_bytes());
    copy_to_user(destination, &bytes)?;
    Ok(0)
}

/// Apply one `setrlimit` request to `old`.
///
/// The rules follow NetBSD `dosetrlimit`: an explicit soft limit above the new hard limit is
/// `EINVAL`, and an unprivileged process may not raise the hard limit (`EPERM`). The one
/// divergence is the BSD "lower the hard limit" form: when the caller passes the current soft
/// value unchanged and lowers the hard below it, the soft value is clamped to the new hard
/// (FreeBSD/OpenBSD clamp; NetBSD rejects). An explicit `cur > max` request remains `EINVAL`.
fn rlimit_set(old: Rlimit, requested: Rlimit, privileged: bool) -> Result<Rlimit, Errno> {
    if requested.cur > requested.max {
        if requested.cur == old.cur {
            if !privileged && requested.max > old.max {
                return Err(Errno::EPERM);
            }
            return Ok(Rlimit::new(requested.max, requested.max));
        }
        return Err(Errno::EINVAL);
    }
    if !privileged && requested.max > old.max {
        return Err(Errno::EPERM);
    }
    Ok(requested)
}

/// `proc_setrlimit(resource, cur, max) -> 0`.
///
/// An unprivileged process may lower either value and may raise the soft limit up to the
/// existing hard limit, but it may not raise the hard limit; only effective uid 0 may do that.
/// `RLIM_INFINITY` is stored verbatim and is subject to the same hard-limit rule.
pub fn setrlimit(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let resource = usize::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    if resource >= RLIMIT_COUNT {
        return Err(Errno::EINVAL);
    }
    let limit = Rlimit {
        cur: frame.rsi,
        max: frame.rdx,
    };
    let privileged = caller.ids.lock().euid == 0;
    let mut limits = caller.limits.lock();
    let slot = limits.get_mut(resource).ok_or(Errno::EINVAL)?;
    *slot = rlimit_set(*slot, limit, privileged)?;
    Ok(0)
}
