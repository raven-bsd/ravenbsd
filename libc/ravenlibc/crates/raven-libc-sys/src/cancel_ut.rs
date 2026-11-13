// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::cancel`].

#![allow(clippy::indexing_slicing)]

use super::*;

use core::sync::atomic::AtomicBool;
use host::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());
static PENDING: AtomicBool = AtomicBool::new(false);
static RESTART: AtomicBool = AtomicBool::new(false);

unsafe fn pending_hook() -> bool {
    PENDING.load(Ordering::Relaxed)
}

unsafe fn eintr_hook() -> RestartAction {
    if RESTART.load(Ordering::Relaxed) {
        RestartAction::Restart
    } else {
        RestartAction::Return
    }
}

fn reset() {
    // SAFETY: clears both slots; no hook is called afterwards.
    unsafe { install_hooks(None, None) };
    PENDING.store(false, Ordering::Relaxed);
    RESTART.store(false, Ordering::Relaxed);
}

#[test]
fn without_hooks_every_family_is_transparent() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    assert_eq!(cancellable(|| Ok(7)), Ok(7));
    assert_eq!(restartable(|| Ok(8)), Ok(8));
    assert_eq!(raw(|| Ok(9)), Ok(9));
    assert_eq!(cancellable(|| Err::<(), _>(Errno::EIO)), Err(Errno::EIO));
    assert!(!pending_cancel());
    assert_eq!(on_eintr(), RestartAction::Return);
}

#[test]
fn pending_hook_blocks_before_the_call() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    // SAFETY: both hooks point at live test statics.
    unsafe { install_hooks(Some(pending_hook), None) };
    PENDING.store(true, Ordering::Relaxed);
    let mut called = false;
    let result = cancellable(|| {
        called = true;
        Ok(1)
    });
    assert_eq!(result, Err(Errno::EINTR));
    assert!(!called);
    reset();
}

#[test]
fn pending_hook_turns_eintr_into_cancellation() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    // SAFETY: both hooks point at live test statics.
    unsafe { install_hooks(Some(pending_hook), None) };
    PENDING.store(true, Ordering::Relaxed);
    assert_eq!(
        cancellable(|| Err::<(), _>(Errno::EINTR)),
        Err(Errno::EINTR)
    );
    reset();
}

#[test]
fn restartable_retries_when_the_hook_says_restart() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    // SAFETY: both hooks point at live test statics.
    unsafe { install_hooks(None, Some(eintr_hook)) };
    RESTART.store(true, Ordering::Relaxed);
    let mut calls = 0u32;
    let result = restartable(|| {
        calls += 1;
        if calls == 1 {
            Err(Errno::EINTR)
        } else {
            Ok(42)
        }
    });
    assert_eq!(result, Ok(42));
    assert_eq!(calls, 2);
    reset();
}

#[test]
fn restartable_returns_eintr_when_the_hook_says_stop() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    // SAFETY: both hooks point at live test statics.
    unsafe { install_hooks(None, Some(eintr_hook)) };
    RESTART.store(false, Ordering::Relaxed);
    let mut calls = 0u32;
    let result = restartable(|| {
        calls += 1;
        Err::<(), _>(Errno::EINTR)
    });
    assert_eq!(result, Err(Errno::EINTR));
    assert_eq!(calls, 1);
    reset();
}

#[test]
fn raw_never_consults_a_hook() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    // SAFETY: both hooks point at live test statics.
    unsafe { install_hooks(Some(pending_hook), Some(eintr_hook)) };
    PENDING.store(true, Ordering::Relaxed);
    RESTART.store(true, Ordering::Relaxed);
    assert_eq!(raw(|| Ok(3)), Ok(3));
    assert_eq!(raw(|| Err::<(), _>(Errno::EINTR)), Err(Errno::EINTR));
    reset();
}
