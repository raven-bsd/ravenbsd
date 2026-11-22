// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! User threads with a guarded stack and a kernel-cleared join word.
//!
//! The thread's `done` word starts at 1 and the kernel clears it to 0 from `THREAD_EXIT`, after
//! the thread is already in kernel mode. `join` may therefore close the handle and unmap the
//! user stack only after it observes zero; the exiting thread can no longer use that stack.

use alloc_crate::boxed::Box;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

/// Page size used for stack alignment and the guard page.
pub(crate) const PAGE: usize = 4096;

struct ThreadControl {
    function: fn(usize),
    arg: usize,
    /// 1 while the thread is live; the kernel writes 0 from `THREAD_EXIT`.
    done: AtomicU32,
}

/// A spawned user thread.
pub struct Thread {
    control: Option<Box<ThreadControl>>,
    handle: Option<Handle>,
    stack_base: usize,
    stack_total: usize,
}

impl Thread {
    /// Wait for the kernel to clear the exit word, release the handle and unmap the stack.
    ///
    /// Zero is published by `THREAD_EXIT` in kernel mode, so the exiting thread has already left
    /// its user stack before the unmap below.
    pub fn join(mut self) -> Result<(), Errno> {
        let Some(control) = self.control.as_ref() else {
            return Err(Errno::EBADF);
        };
        while control.done.load(Ordering::Acquire) != 0 {
            let _ = crate::sys::futex_wait(&control.done, 1, None);
        }
        if let Some(handle) = self.handle.take() {
            crate::sys::handle_close(handle)?;
        }
        let remaining = self.stack_total.saturating_sub(PAGE);
        crate::sys::unmap(
            Handle::SELF_PROC,
            self.stack_base.saturating_add(PAGE),
            remaining,
        )?;
        self.control = None;
        Ok(())
    }

    /// True when the kernel has cleared the exit word.
    pub fn is_finished(&self) -> bool {
        self.control
            .as_ref()
            .is_none_or(|control| control.done.load(Ordering::Acquire) == 0)
    }

    /// The thread handle, if the thread has not been joined.
    pub const fn handle(&self) -> Option<Handle> {
        self.handle
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        // The thread may still be running, so neither its stack nor its control block can be
        // freed here. Closing the handle is safe; an un-joined thread leaks both by design.
        if let Some(handle) = self.handle.take() {
            let _ = crate::sys::handle_close(handle);
        }
        if let Some(control) = self.control.take() {
            Box::leak(control);
        }
    }
}

/// Spawn `function(arg)` on a new user thread.
///
/// `stack_size` is rounded up to a page and one additional guard page is mapped below the stack,
/// then unmapped. The thread joins through a word the kernel clears from `THREAD_EXIT`.
pub fn spawn(function: fn(usize), arg: usize, stack_size: usize) -> Result<Thread, Errno> {
    let (total, guard) = stack_layout(stack_size)?;
    let base = crate::sys::map_anonymous(total)?;
    if let Err(error) = crate::sys::unmap(Handle::SELF_PROC, base, guard) {
        let _ = crate::sys::unmap(Handle::SELF_PROC, base, total);
        return Err(error);
    }
    let stack_top = base.checked_add(total).ok_or(Errno::ENOMEM)?;
    let control = Box::new(ThreadControl {
        function,
        arg,
        done: AtomicU32::new(1),
    });
    let payload = (&*control as *const ThreadControl) as usize;
    let done_address = &control.done as *const AtomicU32 as usize;
    match crate::sys::thread_create_ex(
        Handle::SELF_PROC,
        thread_start as *const () as usize,
        stack_top,
        payload,
        done_address,
    ) {
        Ok(handle) => Ok(Thread {
            control: Some(control),
            handle: Some(handle),
            stack_base: base,
            stack_total: total,
        }),
        Err(error) => {
            let _ = crate::sys::unmap(Handle::SELF_PROC, base.saturating_add(guard), total - guard);
            Err(error)
        }
    }
}

/// Compute the mapped size and guard page for `stack_size`.
pub(crate) fn stack_layout(stack_size: usize) -> Result<(usize, usize), Errno> {
    if stack_size < PAGE {
        return Err(Errno::EINVAL);
    }
    let rounded = stack_size
        .checked_add(PAGE - 1)
        .map(|value| value & !(PAGE - 1))
        .ok_or(Errno::ENOMEM)?;
    let total = rounded.checked_add(PAGE).ok_or(Errno::ENOMEM)?;
    Ok((total, PAGE))
}

extern "C" fn thread_start(payload: usize) -> ! {
    // SAFETY: `spawn` passes a pointer to a `ThreadControl` that lives in the `Thread` until
    // `join` observes the kernel-cleared word; the new thread only reads it while it is alive.
    let control = unsafe { &*(payload as *const ThreadControl) };
    (control.function)(control.arg);
    // `THREAD_EXIT` clears `done` and wakes the joiner from kernel mode. Nothing after this point
    // may touch the user stack, so `join` can unmap it safely.
    crate::sys::thread_exit(0)
}

#[cfg(test)]
#[path = "thread_ut.rs"]
mod tests;
