// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `THREAD_CREATE` and `THREAD_EXIT`.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights, prot};
use raven_usercopy::is_user_address;

use crate::process::{self, ProcessState, handle::Object};
use crate::sched;
use crate::syscall::SyscallFrame;

/// `thread_create(proc, entry, stack_top, arg, flags) -> Thread handle`.
///
/// `proc` is any process handle with `Rights::MAP`; the entry must be inside an executable VMA of
/// that process and the initial stack pointer must lie in a writable VMA. All flag bits are
/// reserved and must be zero.
pub fn create(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let target = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::MAP)? {
            Object::Process(target) => Arc::clone(target),
            _ => return Err(Errno::EBADF),
        }
    };

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

    {
        let address_space = target.address_space.lock();
        let address_space = address_space.as_ref().ok_or(Errno::ESRCH)?;
        let entry_vma = address_space.vmas().find(entry).ok_or(Errno::EFAULT)?;
        if entry_vma.prot() & (prot::EXEC as u8) == 0 {
            return Err(Errno::EACCES);
        }
        let stack_vma = address_space
            .vmas()
            .find(stack_top.saturating_sub(1))
            .ok_or(Errno::EFAULT)?;
        if stack_vma.prot() & (prot::WRITE as u8) == 0 {
            return Err(Errno::EACCES);
        }
        let state = target.state.lock();
        if *state == ProcessState::Exited {
            return Err(Errno::ESRCH);
        }
    }

    let thread = sched::new_user_thread(&target, entry, stack_top, frame.r10)?;
    let rights = Rights::SIGNAL.union(Rights::DUP);
    let raw = caller
        .handles
        .lock()
        .insert(Object::Thread(Arc::clone(&thread)), rights)?;
    if !sched::enqueue(Arc::clone(&thread)) {
        let _ = caller.handles.lock().close(raw);
        return Err(Errno::EAGAIN);
    }
    *target.state.lock() = ProcessState::Running;
    Ok(raw.0 as usize)
}

/// `thread_exit(code) -> !`.
pub fn exit(frame: &SyscallFrame) -> ! {
    sched::exit_thread(frame.rdi as i32)
}
