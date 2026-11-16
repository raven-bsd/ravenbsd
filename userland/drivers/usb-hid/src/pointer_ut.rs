// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the pointer event queue and consumer seam.

use super::*;

fn event(dx: i8, pressed: u8) -> PointerEvent {
    PointerEvent {
        pressed,
        dx,
        ..PointerEvent::NONE
    }
}

#[derive(Default)]
struct Recorder {
    events: [PointerEvent; 4],
    count: usize,
}

impl PointerConsumer for Recorder {
    fn pointer_event(&mut self, event: PointerEvent) {
        if let Some(slot) = self.events.get_mut(self.count) {
            *slot = event;
            self.count = self.count.saturating_add(1);
        }
    }
}

#[test]
fn queue_is_fifo() {
    let mut queue = PointerQueue::new();
    assert!(queue.is_empty());
    assert!(queue.push(event(1, 1)));
    assert!(queue.push(event(-2, 0)));
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.pop(), Some(event(1, 1)));
    assert_eq!(queue.pop(), Some(event(-2, 0)));
    assert_eq!(queue.pop(), None);
    assert!(queue.is_empty());
}

#[test]
fn read_drains_up_to_the_output_length() {
    let mut queue = PointerQueue::default();
    for dx in 0..4 {
        assert!(queue.push(event(dx, 0)));
    }
    let mut out = [PointerEvent::NONE; 2];
    assert_eq!(queue.read(&mut out), 2);
    assert_eq!(out[0].dx, 0);
    assert_eq!(out[1].dx, 1);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.read(&mut out), 2);
    assert_eq!(out[0].dx, 2);
    assert_eq!(out[1].dx, 3);
    assert_eq!(queue.read(&mut out), 0);
}

#[test]
fn a_full_queue_rejects_and_recovers() {
    let mut queue = PointerQueue::new();
    for _ in 0..POINTER_QUEUE {
        assert!(queue.push(event(1, 0)));
    }
    assert_eq!(queue.len(), POINTER_QUEUE);
    assert!(!queue.push(event(2, 0)));
    assert_eq!(queue.len(), POINTER_QUEUE);
    assert_eq!(queue.pop().map(|event| event.dx), Some(1));
    assert!(queue.push(event(2, 0)));
}

#[test]
fn drain_into_hands_events_to_the_consumer() {
    let mut queue = PointerQueue::new();
    assert!(queue.push(event(4, 1)));
    assert!(queue.push(event(-1, 0)));
    let mut recorder = Recorder::default();
    assert_eq!(queue.drain_into(&mut recorder), 2);
    assert_eq!(recorder.count, 2);
    assert_eq!(recorder.events[0].dx, 4);
    assert_eq!(recorder.events[0].pressed, 1);
    assert_eq!(recorder.events[1].dx, -1);
    assert!(queue.is_empty());
    assert_eq!(queue.drain_into(&mut recorder), 0);
}

#[test]
fn clear_drops_every_event() {
    let mut queue = PointerQueue::new();
    assert!(queue.push(event(1, 0)));
    queue.clear();
    assert!(queue.is_empty());
    assert_eq!(queue.pop(), None);
}
