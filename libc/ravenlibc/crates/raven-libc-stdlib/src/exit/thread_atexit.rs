// SPDX-License-Identifier: ISC
// Copyright (c) 2017 Mark Kettenis <kettenis@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/thread_atexit.c
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: thread_atexit.c,v 1.2 2019/06/02 01:03:01 guenther Exp $)
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

//! `__cxa_thread_atexit_impl`: a per-thread LIFO list with a DSO reference.

use alloc::vec::Vec;

use raven_libc_abi::errno::{ENOMEM, Errno};

use super::atexit::Entry;

/// The dynamic-linker seam: `dlctl(NULL, DL_REFERENCE, dso)`.
pub trait DsoRefs {
    /// Take a reference so the object cannot be unloaded while destructors are pending.
    fn add_ref(&mut self, dso: usize);
    /// Drop the reference after the destructor ran.
    fn drop_ref(&mut self, dso: usize);
}

/// The calling thread's pending C++ `thread_local` destructors.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct ThreadAtexit {
    entries: Vec<Entry>,
}

impl ThreadAtexit {
    /// An empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Push a destructor and take a reference on its DSO.
    ///
    /// # Errors
    ///
    /// [`ENOMEM`] when the node cannot be allocated.
    pub fn push<D: DsoRefs>(&mut self, e: Entry, d: &mut D) -> Result<(), Errno> {
        self.entries.try_reserve(1).map_err(|_| errno(ENOMEM))?;
        self.entries.push(e);
        d.add_ref(e.dso);
        Ok(())
    }

    /// Pop the newest destructor and drop its DSO reference.
    pub fn pop<D: DsoRefs>(&mut self, d: &mut D) -> Option<Entry> {
        let entry = self.entries.pop()?;
        d.drop_ref(entry.dso);
        Some(entry)
    }

    /// The number of pending destructors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ThreadAtexit {
    fn default() -> Self {
        Self::new()
    }
}

fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

#[cfg(test)]
#[path = "thread_atexit_ut.rs"]
mod tests;
