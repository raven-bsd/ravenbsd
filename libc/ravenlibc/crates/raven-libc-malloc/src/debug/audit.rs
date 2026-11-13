// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-allocation audit records.

use crate::debug::consts::MAX_STACK;

/// One allocation record.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Record {
    /// User pointer.
    pub user: usize,
    /// Inner allocation base.
    pub base: usize,
    /// Requested size.
    pub size: usize,
    /// Requested alignment.
    pub align: usize,
    /// Allocating thread.
    pub thread: usize,
    /// Monotonic allocation sequence.
    pub seq: u64,
    /// Free sequence, or zero while live.
    pub free_seq: u64,
    /// Live (not yet passed to the inner heap).
    pub live: bool,
    /// In the quarantine ring.
    pub quarantined: bool,
    /// Caller-supplied stack.
    pub stack: [usize; MAX_STACK],
    /// Valid stack words.
    pub stack_len: usize,
}

impl Record {
    /// Whether this record describes `pointer`.
    #[must_use]
    pub fn matches(&self, pointer: usize) -> bool {
        self.user != 0 && self.user == pointer
    }

    /// The first stack word, or zero.
    #[must_use]
    pub fn stack_head(&self) -> usize {
        if self.stack_len == 0 {
            0
        } else {
            self.stack[0]
        }
    }
}
