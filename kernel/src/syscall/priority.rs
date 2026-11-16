// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX priority and scheduler-query syscalls.
//!
//! The raw numbers are kernel-owned: `getpriority`/`setpriority`/`nice` write the signed nice
//! value through a user pointer, so -1 remains a value rather than colliding with the syscall
//! error convention. `sched_get_priority_max/min` and `sched_setscheduler/getscheduler` expose
//! only `SCHED_OTHER`; `SCHED_FIFO`/`SCHED_RR` return `ENOTSUP` instead of pretending the fair
//! scheduler implements a real-time policy.

use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_usercopy::UserSlice;

use crate::process::{self, Process, ProcessState, priority};
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_to_user;

/// Raw `getpriority(which, who, out) -> 0`.
pub const GETPRIORITY: usize = 0x78;
/// Raw `setpriority(which, who, nice) -> 0`.
pub const SETPRIORITY: usize = 0x79;
/// Raw `nice(inc, out) -> 0`.
pub const NICE: usize = 0x7a;
/// Raw `sched_get_priority_max(policy) -> priority`.
pub const SCHED_GET_PRIORITY_MAX: usize = 0x7b;
/// Raw `sched_get_priority_min(policy) -> priority`.
pub const SCHED_GET_PRIORITY_MIN: usize = 0x7c;
/// Raw `sched_setscheduler(pid, policy, priority) -> 0`.
pub const SCHED_SETSCHEDULER: usize = 0x7d;
/// Raw `sched_getscheduler(pid) -> policy`.
pub const SCHED_GETSCHEDULER: usize = 0x7e;

/// `getpriority(which, who, out)`.
pub fn getpriority(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let which = u32::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    let who = frame.rsi as u32 as i32;
    let destination = UserSlice::new(frame.rdx, 4)?;
    let targets = targets(&caller, which, who)?;
    let root = caller.ids.lock().euid == 0;
    for target in &targets {
        if !root && !same_user(&caller, target) {
            return Err(Errno::EPERM);
        }
    }
    let value = targets
        .iter()
        .map(|target| target.nice())
        .min()
        .unwrap_or(0);
    copy_to_user(destination, &value.to_le_bytes())?;
    Ok(0)
}

/// `setpriority(which, who, nice)`.
pub fn setpriority(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let which = u32::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    let who = frame.rsi as u32 as i32;
    let requested = priority::clamp_nice(frame.rdx as u32 as i32);
    let targets = targets(&caller, which, who)?;
    let root = caller.ids.lock().euid == 0;
    for target in &targets {
        if !root && (!same_user(&caller, target) || requested < target.nice()) {
            return Err(Errno::EPERM);
        }
    }
    for target in targets {
        target.set_nice(requested);
    }
    Ok(0)
}

/// `nice(inc, out)`.
pub fn nice(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let increment = frame.rdi as u32 as i32;
    let destination = UserSlice::new(frame.rsi, 4)?;
    let current = caller.nice();
    let sum = i64::from(current).saturating_add(i64::from(increment));
    let requested = if sum < i64::from(priority::NICE_MIN) {
        priority::NICE_MIN
    } else if sum > i64::from(priority::NICE_MAX) {
        priority::NICE_MAX
    } else {
        i32::try_from(sum).unwrap_or(current)
    };
    let value = if caller.ids.lock().euid == 0 || requested >= current {
        requested
    } else {
        // POSIX: an unprivileged attempt to decrease the nice value is ignored.
        current
    };
    caller.set_nice(value);
    copy_to_user(destination, &value.to_le_bytes())?;
    Ok(0)
}

/// `sched_get_priority_max(policy)`.
pub fn sched_get_priority_max(frame: &SyscallFrame) -> Result<usize, Errno> {
    priority_range(frame.rdi)
}

/// `sched_get_priority_min(policy)`.
pub fn sched_get_priority_min(frame: &SyscallFrame) -> Result<usize, Errno> {
    priority_range(frame.rdi)
}

fn priority_range(raw_policy: u64) -> Result<usize, Errno> {
    let policy = u32::try_from(raw_policy).map_err(|_| Errno::EINVAL)?;
    match policy {
        priority::SCHED_OTHER => Ok(0),
        priority::SCHED_FIFO | priority::SCHED_RR => Err(Errno::ENOTSUP),
        _ => Err(Errno::EINVAL),
    }
}

/// `sched_setscheduler(pid, policy, priority)`.
pub fn sched_setscheduler(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let pid = frame.rdi as u32 as i32;
    let policy = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let requested = frame.rdx as u32 as i32;
    let target = target_process(&caller, pid)?;
    match policy {
        priority::SCHED_OTHER => {
            if requested != 0 {
                return Err(Errno::EINVAL);
            }
            let root = caller.ids.lock().euid == 0;
            if !root && !Arc::ptr_eq(&caller, &target) {
                return Err(Errno::EPERM);
            }
            Ok(0)
        }
        priority::SCHED_FIFO | priority::SCHED_RR => Err(Errno::ENOTSUP),
        _ => Err(Errno::EINVAL),
    }
}

/// `sched_getscheduler(pid)`.
pub fn sched_getscheduler(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let pid = frame.rdi as u32 as i32;
    let target = target_process(&caller, pid)?;
    let root = caller.ids.lock().euid == 0;
    if !root && !same_user(&caller, &target) {
        return Err(Errno::EPERM);
    }
    Ok(priority::SCHED_OTHER as usize)
}

fn targets(caller: &Arc<Process>, which: u32, who: i32) -> Result<Vec<Arc<Process>>, Errno> {
    let mut result = Vec::new();
    match which {
        priority::PRIO_PROCESS => result.push(target_process(caller, who)?),
        priority::PRIO_GROUP => {
            let group = if who == 0 {
                caller.ids.lock().pgid
            } else {
                u64::try_from(who).map_err(|_| Errno::ESRCH)?
            };
            for candidate in process::processes() {
                if *candidate.state.lock() == ProcessState::Exited {
                    continue;
                }
                if candidate.ids.lock().pgid == group {
                    result.push(candidate);
                }
            }
        }
        priority::PRIO_USER => {
            let uid = if who == 0 {
                caller.ids.lock().uid
            } else {
                u32::try_from(who).map_err(|_| Errno::ESRCH)?
            };
            for candidate in process::processes() {
                if *candidate.state.lock() == ProcessState::Exited {
                    continue;
                }
                if candidate.ids.lock().uid == uid {
                    result.push(candidate);
                }
            }
        }
        _ => return Err(Errno::EINVAL),
    }
    if result.is_empty() {
        return Err(Errno::ESRCH);
    }
    Ok(result)
}

fn target_process(caller: &Arc<Process>, pid: i32) -> Result<Arc<Process>, Errno> {
    if pid == 0 {
        return Ok(Arc::clone(caller));
    }
    let pid = u64::try_from(pid).map_err(|_| Errno::ESRCH)?;
    let target = process::lookup_pid(pid).ok_or(Errno::ESRCH)?;
    if *target.state.lock() == ProcessState::Exited {
        return Err(Errno::ESRCH);
    }
    Ok(target)
}

fn same_user(caller: &Process, target: &Process) -> bool {
    let caller_ids = *caller.ids.lock();
    let target_ids = *target.ids.lock();
    (target_ids.uid == caller_ids.uid && target_ids.euid == caller_ids.euid)
        || (target_ids.uid == caller_ids.euid && target_ids.euid == caller_ids.uid)
}
