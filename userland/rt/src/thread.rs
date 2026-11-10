// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! User threads with a guarded stack and a futex join word.

use alloc_crate::boxed::Box;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

/// Page size used for stack alignment and the guard page.
pub(crate) const PAGE: usize = 4096;

struct ThreadControl {
    function: fn(usize),
    arg: usize,
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
    /// Wait for the thread to finish, release its handle and unmap its stack.
    pub fn join(mut self) -> Result<(), Errno> {
        let Some(control) = self.control.as_ref() else {
            return Err(Errno::EBADF);
        };
        while control.done.load(Ordering::Acquire) == 0 {
            let _ = crate::sys::futex_wait(&control.done, 0, None);
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

    /// True when the thread function has returned.
    pub fn is_finished(&self) -> bool {
        self.control
            .as_ref()
            .is_none_or(|control| control.done.load(Ordering::Acquire) != 0)
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
/// then unmapped. The thread joins through a futex word set just before `THREAD_EXIT`.
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
        done: AtomicU32::new(0),
    });
    let payload = (&*control as *const ThreadControl) as usize;
    match crate::sys::thread_create(
        Handle::SELF_PROC,
        thread_start as *const () as usize,
        stack_top,
        payload,
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
    // `join` observes the done word; the new thread only reads it while it is alive.
    let control = unsafe { &*(payload as *const ThreadControl) };
    (control.function)(control.arg);
    control.done.store(1, Ordering::Release);
    let _ = crate::sys::futex_wake(&control.done, u32::MAX);
    crate::sys::thread_exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_layout_aligns_and_adds_a_guard() {
        assert_eq!(stack_layout(4096).unwrap(), (8192, 4096));
        assert_eq!(stack_layout(4097).unwrap(), (12288, 4096));
        assert_eq!(stack_layout(16 * 1024).unwrap(), (20 * 1024, 4096));
    }

    #[test]
    fn stack_layout_rejects_tiny_stacks() {
        assert_eq!(stack_layout(0), Err(Errno::EINVAL));
        assert_eq!(stack_layout(PAGE - 1), Err(Errno::EINVAL));
    }

    #[test]
    fn stack_layout_rejects_overflow() {
        assert_eq!(stack_layout(usize::MAX), Err(Errno::ENOMEM));
    }

    #[test]
    fn control_done_word_starts_clear() {
        let control = ThreadControl {
            function: |_| {},
            arg: 0,
            done: AtomicU32::new(0),
        };
        assert_eq!(control.done.load(Ordering::Acquire), 0);
        control.done.store(1, Ordering::Release);
        assert_eq!(control.done.load(Ordering::Acquire), 1);
    }
}
