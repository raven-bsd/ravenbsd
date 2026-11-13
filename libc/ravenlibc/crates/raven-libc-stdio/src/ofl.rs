// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/ofl.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// The donor file has no per-file header; it is covered by musl/COPYRIGHT (lines 1-24).
// MIT via musl/COPYRIGHT.
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//
//! The open-file list and its raw lock (`musl:src/stdio/ofl.c`, `ofl_add.c`).
//!
//! The lock is a plain atomic word with [`crate::Lock`] waits, usable before threads exist and
//! inside the fork handlers (R43). Lock order (`donor-map/09` 3.8) is list lock (group 5)
//! before any FILE lock (group 6); the list lock is never held while a FILE lock is acquired
//! and released again, except in `fflush(NULL)`/`stdio_cleanup`, which hold the list lock and
//! take one FILE lock at a time (trap S6).
//!
//! Links are `Option<&'static File>` because every FILE lives in static or leaked storage
//! (trap S3); the enum state a `fclose` leaves behind keeps the buffer empty so list walkers
//! can skip a dead FILE (`fclose.c:16-22`).

use core::sync::atomic::Ordering;

use crate::file::File;
use crate::state::State;
use crate::traits::Env;

/// The open-file list held locked.
pub(crate) struct OflGuard<'a, E: Env> {
    state: &'a State<E>,
}

impl<'a, E: Env> OflGuard<'a, E> {
    /// Take the list lock.
    pub(crate) fn acquire(state: &'a State<E>) -> Self {
        let env = state.env();
        let word = state.ofl_lock_word();
        loop {
            if word
                .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
            env.wait(word, 1);
        }
        #[cfg(test)]
        crate::fake_ut::order::list_acquired();
        Self { state }
    }

    /// The list head.
    pub(crate) fn head(&self) -> Option<&'static File<E>> {
        self.state.ofl_head()
    }
}

impl<E: Env> Drop for OflGuard<'_, E> {
    fn drop(&mut self) {
        let word = self.state.ofl_lock_word();
        word.store(0, Ordering::Release);
        #[cfg(test)]
        crate::fake_ut::order::list_released();
        self.state.env().wake(word, 1);
    }
}

/// Take the list lock and forget the guard (the fork prepare handler).
pub(crate) fn lock_list<E: Env>(state: &State<E>) {
    let env = state.env();
    let word = state.ofl_lock_word();
    loop {
        if word
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            #[cfg(test)]
            crate::fake_ut::order::list_acquired();
            return;
        }
        env.wait(word, 1);
    }
}

/// Release the list lock and wake a waiter (the fork parent handler).
pub(crate) fn unlock_list<E: Env>(state: &State<E>) {
    let word = state.ofl_lock_word();
    word.store(0, Ordering::Release);
    #[cfg(test)]
    crate::fake_ut::order::list_released();
    state.env().wake(word, 1);
}

/// Reset the list lock to 0 without waking (the fork child handler; `fork.c:80`).
pub(crate) fn reset_list<E: Env>(state: &State<E>) {
    state.ofl_lock_word().store(0, Ordering::Release);
}

/// Add a new FILE at the head of the list (`__ofl_add`).
pub(crate) fn add<E: Env>(state: &'static State<E>, file: &'static File<E>) {
    let guard = OflGuard::acquire(state);
    let head = guard.head();
    {
        let st = file.st_mut();
        st.prev = None;
        st.next = head;
    }
    if let Some(old_head) = head {
        old_head.st_mut().prev = Some(file);
    }
    state.set_ofl_head(Some(file));
}

/// Remove a FILE from the list; the caller holds the list lock.
pub(crate) fn remove_locked<E: Env>(state: &'static State<E>, file: &'static File<E>) {
    let (prev, next) = {
        let st = file.st();
        (st.prev, st.next)
    };
    if let Some(previous) = prev {
        previous.st_mut().next = next;
    } else if state
        .ofl_head()
        .is_some_and(|head| core::ptr::eq(head, file))
    {
        state.set_ofl_head(next);
    }
    if let Some(following) = next {
        following.st_mut().prev = prev;
    }
    let st = file.st_mut();
    st.prev = None;
    st.next = None;
}

/// Remove a FILE from the list, taking the list lock.
pub(crate) fn remove<E: Env>(state: &'static State<E>, file: &'static File<E>) {
    let _guard = OflGuard::acquire(state);
    remove_locked(state, file);
}

#[cfg(test)]
#[path = "ofl_ut.rs"]
mod tests;
