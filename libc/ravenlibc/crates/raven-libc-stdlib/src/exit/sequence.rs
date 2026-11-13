// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/exit.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: exit.c,v 1.15 2015/09/13 08:31:47 guenther Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

#![allow(clippy::arithmetic_side_effects)] // RavenBSD: restart guard counter

//! The `exit(status)` order as a pure function.
//!
//! A-stdlib-8: (1) the calling thread's `__cxa_thread_atexit` list LIFO; (2) every table entry
//! newest first (C++ static destructors and `atexit` handlers interleave in that one order); the
//! loader's finaliser is simply the entry registered first; the stdio cleanup was registered at
//! the oldest position and therefore runs last; then `_exit` (the shell). `_Exit.c` is the
//! shell's single `_exit` call and is not translated. Signal handlers and atfork entries are
//! other crates' hooks; this module documents the seam only.

use alloc::vec::Vec;

use super::atexit::Entry;
use super::thread_atexit::{DsoRefs, ThreadAtexit};

/// Run the thread list first, then drain the table; returns the order.
///
/// Handlers registered while this loop runs are picked up by `next_to_run`'s restart behaviour
/// (the table has no terminal iterator). The caller then calls `_exit(status)`.
pub fn exit_steps<D: DsoRefs>(
    table: &mut super::atexit::ExitTable,
    thread: &mut ThreadAtexit,
    dso_refs: &mut D,
) -> Vec<Entry> {
    let mut steps = Vec::new();
    while let Some(entry) = thread.pop(dso_refs) {
        steps.push(entry);
    }
    let mut guard = 0usize;
    while let Some(entry) = table.next_to_run() {
        steps.push(entry);
        guard = guard.wrapping_add(1);
        if guard > 1_000_000 {
            break;
        }
    }
    steps
}

#[cfg(test)]
#[path = "sequence_ut.rs"]
mod tests;
