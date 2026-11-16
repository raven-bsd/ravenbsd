// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded pointer event queue and the consumer seam.

use raven_usb_hid::PointerEvent;

/// Capacity of the mouse-to-pointer queue.
pub const POINTER_QUEUE: usize = 64;

/// A single-reader queue of decoded pointer events.
pub struct PointerQueue {
    buffer: [PointerEvent; POINTER_QUEUE],
    head: usize,
    tail: usize,
    count: usize,
}

impl PointerQueue {
    /// An empty queue.
    pub const fn new() -> Self {
        Self {
            buffer: [PointerEvent::NONE; POINTER_QUEUE],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    /// Append one event; false when the queue is full.
    pub fn push(&mut self, event: PointerEvent) -> bool {
        if self.count >= POINTER_QUEUE {
            return false;
        }
        if let Some(slot) = self.buffer.get_mut(self.head) {
            *slot = event;
        }
        self.head = (self.head.saturating_add(1)) % POINTER_QUEUE;
        self.count = self.count.saturating_add(1);
        true
    }

    /// Remove one event.
    pub fn pop(&mut self) -> Option<PointerEvent> {
        if self.count == 0 {
            return None;
        }
        let event = self.buffer.get(self.tail).copied();
        self.tail = (self.tail.saturating_add(1)) % POINTER_QUEUE;
        self.count = self.count.saturating_sub(1);
        event
    }

    /// Drain up to `out.len()` events; returns the count.
    pub fn read(&mut self, out: &mut [PointerEvent]) -> usize {
        let mut count = 0usize;
        for slot in out.iter_mut() {
            let Some(event) = self.pop() else {
                break;
            };
            *slot = event;
            count = count.saturating_add(1);
        }
        count
    }

    /// Hand every waiting event to `consumer`; returns the count.
    pub fn drain_into<C: PointerConsumer>(&mut self, consumer: &mut C) -> usize {
        let mut count = 0usize;
        while let Some(event) = self.pop() {
            consumer.pointer_event(event);
            count = count.saturating_add(1);
        }
        count
    }

    /// Events waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when no event is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Drop every waiting event.
    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.count = 0;
    }
}

impl Default for PointerQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// The pointer/graphics consumer seam.
///
/// A console pointer or windowing server implements this to receive events without owning the
/// queue; [`PointerQueue::drain_into`] and [`crate::MouseService::drain_pointer`] are the
/// adapters.
pub trait PointerConsumer {
    /// Handle one pointer event.
    fn pointer_event(&mut self, event: PointerEvent);
}

#[cfg(test)]
#[path = "pointer_ut.rs"]
mod tests;
