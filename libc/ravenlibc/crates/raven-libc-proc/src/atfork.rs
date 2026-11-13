// SPDX-License-Identifier: BSD-2-Clause
// Copyright (C) 2008 The Android Open Source Project
// All rights reserved.
//
// RavenBSD: transpiled from bionic:libc/bionic/pthread_atfork.cpp @ 731631f300090436d7f5df80d50b6275c8c60a93
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
//  * Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
//  * Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in
//    the documentation and/or other materials provided with the
//    distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
// FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
// COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
// BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS
// OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
// AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
// OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! The `pthread_atfork` registry.
//!
//! The donor is `bionic:libc/bionic/pthread_atfork.cpp` (BSD-2). The list order, the reverse
//! prepare and forward parent/child calls, the lock lifetime and the child reset are kept.

use alloc::vec::Vec;

use crate::errors::{ENOMEM, Errno};
use crate::limits::MAX_ATFORK;
use crate::lock_registry::ForkLock;

/// One registered handler set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Prepare handler address, zero for none.
    pub prepare: usize,
    /// Parent handler address, zero for none.
    pub parent: usize,
    /// Child handler address, zero for none.
    pub child: usize,
    /// Owning DSO handle, zero for the executable.
    pub dso: usize,
}

/// The atfork registry.
#[derive(Clone, Default)]
pub struct Registry {
    entries: Vec<Entry>,
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Register one handler set.
    pub fn register(
        &mut self,
        prepare: usize,
        parent: usize,
        child: usize,
        dso: usize,
    ) -> Result<(), Errno> {
        if self.entries.len() >= MAX_ATFORK {
            return Err(ENOMEM);
        }
        self.entries.push(Entry {
            prepare,
            parent,
            child,
            dso,
        });
        Ok(())
    }

    /// Remove every entry owned by `dso`.
    pub fn unregister(&mut self, dso: usize) {
        self.entries.retain(|entry| entry.dso != dso);
    }

    /// Run prepare handlers in reverse order while holding the registry lock.
    pub fn run_prepare(&self, lock: &dyn ForkLock, call: &mut dyn FnMut(&Entry)) {
        if self.entries.is_empty() {
            return;
        }
        lock.lock();
        for entry in self.entries.iter().rev() {
            call(entry);
        }
    }

    /// Run parent handlers in forward order, then unlock the registry.
    pub fn run_parent(&self, lock: &dyn ForkLock, call: &mut dyn FnMut(&Entry)) {
        if self.entries.is_empty() {
            return;
        }
        for entry in &self.entries {
            call(entry);
        }
        lock.unlock();
    }

    /// Run child handlers in forward order, then reset the registry lock.
    pub fn run_child(&self, lock: &dyn ForkLock, call: &mut dyn FnMut(&Entry)) {
        if self.entries.is_empty() {
            return;
        }
        for entry in &self.entries {
            call(entry);
        }
        lock.reset();
    }

    /// Number of registered entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no handler is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[path = "atfork_ut.rs"]
mod tests;
