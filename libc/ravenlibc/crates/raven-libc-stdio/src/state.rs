// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The per-process crate state: the environment, the open-file-list lock and head, the
//! one-shot exit-cleanup registration, and the three standard-stream slots.
//!
//! musl keeps the list head and its lock as file-scope statics (`ofl.c`, `__stdio_exit.c`);
//! here they live in one value the shell creates (one per libc instance, which makes the
//! fake-environment tests independent). `mark_used` replaces the `__stdio_exit_needed`
//! weak-symbol trick: it returns `true` exactly once, and the shell registers the cleanup
//! with C41 `register_oldest` (RD-new 2).

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};

use crate::file::File;
use crate::traits::Env;

/// The three standard streams, for the `fflush(NULL)` order and the exit cleanup.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum StdKind {
    /// `stdin`.
    In,
    /// `stdout`.
    Out,
    /// `stderr`.
    Err,
}

/// One libc instance's `FILE` state.
pub struct State<E: Env> {
    env: E,
    ofl_lock: AtomicI32,
    ofl_head: AtomicPtr<File<E>>,
    used: AtomicBool,
    stdin: AtomicPtr<File<E>>,
    stdout: AtomicPtr<File<E>>,
    stderr: AtomicPtr<File<E>>,
}

impl<E: Env> State<E> {
    /// A fresh state; usable in a `static` initialiser.
    pub const fn new(env: E) -> Self {
        Self {
            env,
            ofl_lock: AtomicI32::new(0),
            ofl_head: AtomicPtr::new(core::ptr::null_mut()),
            used: AtomicBool::new(false),
            stdin: AtomicPtr::new(core::ptr::null_mut()),
            stdout: AtomicPtr::new(core::ptr::null_mut()),
            stderr: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// The platform environment.
    #[must_use]
    pub fn env(&self) -> &E {
        &self.env
    }

    /// The one-shot exit-cleanup registration.
    ///
    /// Returns `true` exactly once; every later call returns `false`.
    pub fn mark_used(&self) -> bool {
        self.used
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Whether the cleanup was registered.
    #[must_use]
    pub fn is_used(&self) -> bool {
        self.used.load(Ordering::Acquire)
    }

    /// The open-file-list lock word.
    #[must_use]
    pub(crate) fn ofl_lock_word(&self) -> &AtomicI32 {
        &self.ofl_lock
    }

    /// The list head.
    #[must_use]
    pub(crate) fn ofl_head(&self) -> Option<&'static File<E>> {
        let pointer = self.ofl_head.load(Ordering::Acquire);
        if pointer.is_null() {
            None
        } else {
            Some(crate::raw::file_ref(pointer.cast_const()))
        }
    }

    /// Replace the list head.
    pub(crate) fn set_ofl_head(&self, head: Option<&'static File<E>>) {
        let pointer = match head {
            Some(file) => (file as *const File<E>).cast_mut(),
            None => core::ptr::null_mut(),
        };
        self.ofl_head.store(pointer, Ordering::Release);
    }

    /// Record one standard stream (idempotent, first writer wins).
    pub(crate) fn bind_std(&self, kind: StdKind, file: &'static File<E>) {
        let slot = match kind {
            StdKind::In => &self.stdin,
            StdKind::Out => &self.stdout,
            StdKind::Err => &self.stderr,
        };
        let pointer = (file as *const File<E>).cast_mut();
        let mut current = slot.load(Ordering::Acquire);
        loop {
            if !current.is_null() {
                return;
            }
            match slot.compare_exchange(current, pointer, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return,
                Err(observed) => current = observed,
            }
        }
    }

    /// A previously bound standard stream.
    #[must_use]
    pub(crate) fn std_file(&self, kind: StdKind) -> Option<&'static File<E>> {
        let slot = match kind {
            StdKind::In => &self.stdin,
            StdKind::Out => &self.stdout,
            StdKind::Err => &self.stderr,
        };
        let pointer = slot.load(Ordering::Acquire);
        if pointer.is_null() {
            None
        } else {
            Some(crate::raw::file_ref(pointer.cast_const()))
        }
    }
}

#[cfg(test)]
#[path = "state_ut.rs"]
mod tests;
