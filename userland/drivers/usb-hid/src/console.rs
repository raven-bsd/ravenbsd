// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded console input queue.

/// Capacity of the keyboard-to-console queue.
pub const CONSOLE_QUEUE: usize = 256;

/// A single-reader byte queue fed by the keyboard service.
pub struct ConsoleInput {
    buffer: [u8; CONSOLE_QUEUE],
    head: usize,
    tail: usize,
    count: usize,
}

impl ConsoleInput {
    /// An empty queue.
    pub const fn new() -> Self {
        Self {
            buffer: [0; CONSOLE_QUEUE],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    /// Append one byte; false when the queue is full.
    pub fn push(&mut self, byte: u8) -> bool {
        if self.count >= CONSOLE_QUEUE {
            return false;
        }
        if let Some(slot) = self.buffer.get_mut(self.head) {
            *slot = byte;
        }
        self.head = (self.head.saturating_add(1)) % CONSOLE_QUEUE;
        self.count = self.count.saturating_add(1);
        true
    }

    /// Remove one byte.
    pub fn pop(&mut self) -> Option<u8> {
        if self.count == 0 {
            return None;
        }
        let byte = self.buffer.get(self.tail).copied();
        self.tail = (self.tail.saturating_add(1)) % CONSOLE_QUEUE;
        self.count = self.count.saturating_sub(1);
        byte
    }

    /// Drain up to `out.len()` bytes; returns the count.
    pub fn read(&mut self, out: &mut [u8]) -> usize {
        let mut count = 0usize;
        for slot in out.iter_mut() {
            let Some(byte) = self.pop() else {
                break;
            };
            *slot = byte;
            count = count.saturating_add(1);
        }
        count
    }

    /// Bytes waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when no byte is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Drop every waiting byte.
    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.count = 0;
    }
}

impl Default for ConsoleInput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "console_ut.rs"]
mod tests;
