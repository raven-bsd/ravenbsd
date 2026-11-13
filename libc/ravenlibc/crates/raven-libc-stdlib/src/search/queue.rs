// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1993 John Brezak
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/insque.c and remque.c
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: insque.c,v 1.3 2005/08/08 08:05:36 espie Exp $,
// $OpenBSD: remque.c,v 1.3 2005/08/08 08:05:36 espie Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. The name of the author may not be used to endorse or promote products
//    derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS OR
// IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES
// OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.
// IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
// NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
// THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! `insque`/`remque` over a [`ListStore`] of opaque nodes.

/// The doubly linked list seam.
pub trait ListStore {
    /// The opaque element handle; the shell maps it to `struct qelem *`.
    type Node: Copy + Eq;

    /// The forward link.
    fn next(&self, node: Self::Node) -> Option<Self::Node>;
    /// The backward link.
    fn prev(&self, node: Self::Node) -> Option<Self::Node>;
    /// Set the forward link.
    fn set_next(&mut self, node: Self::Node, next: Option<Self::Node>);
    /// Set the backward link.
    fn set_prev(&mut self, node: Self::Node, prev: Option<Self::Node>);
}

/// `insque(entry, pred)`; `pred == None` makes a one-element list with both links `NULL`.
pub fn insque<S: ListStore>(store: &mut S, entry: S::Node, pred: Option<S::Node>) {
    match pred {
        None => {
            store.set_next(entry, None);
            store.set_prev(entry, None);
        }
        Some(pred) => {
            let next = store.next(pred);
            store.set_next(entry, next);
            store.set_prev(entry, Some(pred));
            if let Some(next) = next {
                store.set_prev(next, Some(entry));
            }
            store.set_next(pred, Some(entry));
        }
    }
}

/// `remque(element)`; leaves the removed element's own links untouched.
pub fn remque<S: ListStore>(store: &mut S, element: S::Node) {
    if let Some(next) = store.next(element) {
        let prev = store.prev(element);
        store.set_prev(next, prev);
    }
    if let Some(prev) = store.prev(element) {
        let next = store.next(element);
        store.set_next(prev, next);
    }
}

#[cfg(test)]
#[path = "queue_ut.rs"]
mod tests;
