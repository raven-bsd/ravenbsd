// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996, 1997, 1998, 1999, 2000 John D. Polstra.
// Copyright 2003 Alexander Kabaev <kan@FreeBSD.ORG>.
// Copyright 2009-2013 Konstantin Belousov <kib@FreeBSD.ORG>.
// Copyright 2012 John Marino <draco@marino.st>.
// Copyright 2014-2017 The FreeBSD Foundation
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld_lock.c @
// 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: 133344405f532bd54ff3629a77d4c4e062f01fdb)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! The three loader locks: `bind`, `phdr` and `libc`.
//!
//! Before threads exist a plain flag is used; the futex-backed implementation lives in
//! `raw/lock_futex.rs` and is selected by `cfg(target_os = "none")`.

/// One lock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockState {
    held: bool,
    owner: u32,
    waits: u32,
}

impl LockState {
    /// An unlocked state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            held: false,
            owner: 0,
            waits: 0,
        }
    }

    /// Try to take the lock for `owner`.
    pub fn try_acquire(&mut self, owner: u32) -> bool {
        if self.held {
            self.waits = self.waits.saturating_add(1);
            false
        } else {
            self.held = true;
            self.owner = owner;
            true
        }
    }

    /// Release the lock; false when it was not held by `owner`.
    pub fn release(&mut self, owner: u32) -> bool {
        if !self.held || self.owner != owner {
            return false;
        }
        self.held = false;
        self.owner = 0;
        true
    }

    /// Whether the lock is held.
    #[must_use]
    pub fn is_held(&self) -> bool {
        self.held
    }

    /// The current owner.
    #[must_use]
    pub fn owner(&self) -> u32 {
        self.owner
    }

    /// The number of failed acquisitions.
    #[must_use]
    pub fn waits(&self) -> u32 {
        self.waits
    }
}

/// The loader's three locks.
pub struct RtldLock {
    /// The relocation/bind lock.
    pub bind: LockState,
    /// The program-header list lock.
    pub phdr: LockState,
    /// The libc interposition lock.
    pub libc: LockState,
}

impl RtldLock {
    /// All three locks unlocked.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bind: LockState::new(),
            phdr: LockState::new(),
            libc: LockState::new(),
        }
    }
}

impl Default for RtldLock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "lock_ut.rs"]
mod tests;
