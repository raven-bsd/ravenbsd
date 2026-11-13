// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thin cancellation-point wrappers (feature `cancel`).
//!
//! The algorithmic cancellation state belongs to `raven-libc-thread` (C57). This module owns the
//! two hook slots that C57 installs: `pending_cancel() -> bool` and
//! `on_eintr() -> RestartAction`. With no hooks installed every family is transparent.

use core::sync::atomic::{AtomicUsize, Ordering};

use raven_abi::errno::Errno;

/// What the wrapper does after a blocking call returned `EINTR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartAction {
    /// The interrupted call is in class R and every handler had `SA_RESTART`: retry it.
    Restart,
    /// Return `EINTR` to the caller.
    Return,
}

/// A `pending_cancel()` hook installed by the threads area.
pub type PendingCancel = unsafe fn() -> bool;
/// An `on_eintr()` hook installed by the signal/threads area.
pub type OnEintr = unsafe fn() -> RestartAction;

static PENDING_HOOK: AtomicUsize = AtomicUsize::new(0);
static EINTR_HOOK: AtomicUsize = AtomicUsize::new(0);

/// Install or clear the cancellation hooks.
///
/// # Safety
///
/// The function pointers must stay valid for the life of the process and must not allocate or
/// re-enter the wrapper they are called from. Passing `None` clears the slot.
pub unsafe fn install_hooks(pending: Option<PendingCancel>, on_eintr: Option<OnEintr>) {
    PENDING_HOOK.store(pending.map_or(0, |hook| hook as usize), Ordering::Release);
    EINTR_HOOK.store(on_eintr.map_or(0, |hook| hook as usize), Ordering::Release);
}

/// Whether a cancellation request is pending; `false` without a hook.
#[inline]
pub fn pending_cancel() -> bool {
    let hook = PENDING_HOOK.load(Ordering::Acquire);
    if hook == 0 {
        return false;
    }
    // SAFETY: only `install_hooks` stores a non-zero value, and its contract guarantees the
    // function pointer is valid for the process lifetime.
    unsafe {
        let hook: PendingCancel = core::mem::transmute(hook);
        hook()
    }
}

/// What to do after `EINTR`; [`RestartAction::Return`] without a hook.
#[inline]
pub fn on_eintr() -> RestartAction {
    let hook = EINTR_HOOK.load(Ordering::Acquire);
    if hook == 0 {
        return RestartAction::Return;
    }
    // SAFETY: see [`pending_cancel`].
    unsafe {
        let hook: OnEintr = core::mem::transmute(hook);
        hook()
    }
}

/// Wrap a blocking call in the cancellation check (class R/N boundary).
#[inline]
pub fn cancellable<T>(mut f: impl FnMut() -> Result<T, Errno>) -> Result<T, Errno> {
    if pending_cancel() {
        return Err(Errno::EINTR);
    }
    let result = f();
    if matches!(result, Err(Errno::EINTR)) && pending_cancel() {
        return Err(Errno::EINTR);
    }
    result
}

/// Wrap a class R call: retry `EINTR` when the installed hook says so.
#[inline]
pub fn restartable<T>(mut f: impl FnMut() -> Result<T, Errno>) -> Result<T, Errno> {
    loop {
        match f() {
            Err(Errno::EINTR) if matches!(on_eintr(), RestartAction::Restart) => continue,
            other => return other,
        }
    }
}

/// Call `f` without cancellation or restart handling (the `*_nocancel` idea).
#[inline]
pub fn raw<T>(f: impl FnOnce() -> Result<T, Errno>) -> Result<T, Errno> {
    f()
}

#[cfg(test)]
#[path = "cancel_ut.rs"]
mod tests;
