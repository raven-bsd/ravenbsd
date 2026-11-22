// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only reference for the TCP usrreq/listen/accept/close path.
//!
//! The queue capacity follows FreeBSD `sys/kern/uipc_socket.c`:
//! `solisten_proto` clamps the backlog to `somaxconn`, and `solisten_clone`
//! refuses a child when the completed queue would exceed `3 * backlog / 2 + 1`.
//! The reference is independent of the product queue and is driven by the
//! public `Stack` test hooks in the tests.

#![forbid(unsafe_code)]

use std::collections::VecDeque;

/// The product's `ACCEPT_QUEUE_CEILING` analogue.
pub const ACCEPT_QUEUE_CEILING: usize = 65_535;

/// Completed-connection capacity for a listener backlog.
#[must_use]
pub fn completed_capacity(backlog: usize) -> usize {
    let backlog = backlog.clamp(1, ACCEPT_QUEUE_CEILING);
    backlog
        .saturating_mul(3)
        .saturating_div(2)
        .saturating_add(1)
        .min(ACCEPT_QUEUE_CEILING)
}

/// A reference listener queue.
#[derive(Clone, Debug)]
pub struct RefListener {
    capacity: usize,
    queue: VecDeque<u32>,
    closed: bool,
}

impl RefListener {
    /// Build a listener reference for `backlog`.
    #[must_use]
    pub fn new(backlog: usize) -> Self {
        Self {
            capacity: completed_capacity(backlog),
            queue: VecDeque::new(),
            closed: false,
        }
    }

    /// The completed-queue capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Add one completed connection; false when full.
    pub fn push(&mut self, socket: u32) -> bool {
        if self.closed || self.queue.len() >= self.capacity {
            return false;
        }
        self.queue.push_back(socket);
        true
    }

    /// Pop the oldest completed connection.
    pub fn accept(&mut self) -> Option<u32> {
        if self.closed {
            return None;
        }
        self.queue.pop_front()
    }

    /// Close the listener; all queued entries are abandoned.
    pub fn close(&mut self) {
        self.closed = true;
        self.queue.clear();
    }

    /// Whether the listener is closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Queued sockets, oldest first.
    #[must_use]
    pub fn queued(&self) -> Vec<u32> {
        self.queue.iter().copied().collect()
    }

    /// Queue length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
