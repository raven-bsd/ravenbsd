// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure registration and readiness state for RavenBSD event queues.

#![cfg_attr(not(target_os = "none"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;

/// A source is readable.
pub const READABLE: u32 = 1;
/// A source is writable.
pub const WRITABLE: u32 = 2;
/// A source has hung up.
pub const HUP: u32 = 4;
/// A notification-style source fired.
pub const SIGNALED: u32 = 8;
/// Report transitions rather than current readiness.
pub const EDGE: u32 = 0x8000_0000;
/// All event bits that may be registered.
pub const EVENT_MASK: u32 = READABLE | WRITABLE | HUP | SIGNALED;
/// Maximum registrations in one event queue.
pub const MAX_REGISTRATIONS: usize = 4096;

/// A ready event returned to the kernel wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ready {
    /// User-provided opaque value.
    pub cookie: u64,
    /// Ready bits matching the registration mask.
    pub events: u32,
}

/// One source selected for delivery, including its internal handle key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delivery {
    /// Handle value used as the registration key.
    pub key: u32,
    /// Public event record.
    pub ready: Ready,
}

/// Errors from registration operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The event mask is empty or contains unknown bits.
    InvalidMask,
    /// The operation would create a duplicate registration.
    Exists,
    /// The registration does not exist.
    Missing,
    /// The registration table is full.
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Registration {
    key: u32,
    events: u32,
    cookie: u64,
    edge: bool,
    seen: u32,
}

/// Bounded registrations with independent edge/level bookkeeping.
#[derive(Default)]
pub struct Queue {
    registrations: Vec<Registration>,
}

impl Queue {
    /// Make an empty queue.
    pub const fn new() -> Self {
        Self {
            registrations: Vec::new(),
        }
    }

    /// Number of active registrations.
    #[allow(dead_code)] // The kernel inclusion needs only mutation; host users query this state.
    pub fn len(&self) -> usize {
        self.registrations.len()
    }

    /// True when no registrations exist.
    #[allow(dead_code)] // The kernel inclusion needs only mutation; host users query this state.
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }

    /// Add a handle registration.
    pub fn add(&mut self, key: u32, events: u32, cookie: u64) -> Result<(), Error> {
        let (mask, edge) = validate(events)?;
        if self.registrations.iter().any(|entry| entry.key == key) {
            return Err(Error::Exists);
        }
        if self.registrations.len() == MAX_REGISTRATIONS {
            return Err(Error::Full);
        }
        self.registrations.push(Registration {
            key,
            events: mask,
            cookie,
            edge,
            seen: 0,
        });
        Ok(())
    }

    /// Replace a registration's filter and cookie.
    pub fn modify(&mut self, key: u32, events: u32, cookie: u64) -> Result<(), Error> {
        let (mask, edge) = validate(events)?;
        let entry = self
            .registrations
            .iter_mut()
            .find(|entry| entry.key == key)
            .ok_or(Error::Missing)?;
        entry.events = mask;
        entry.cookie = cookie;
        entry.edge = edge;
        entry.seen = 0;
        Ok(())
    }

    /// Delete a registration. Later observations cannot return its cookie.
    pub fn delete(&mut self, key: u32) -> Result<(), Error> {
        let index = self
            .registrations
            .iter()
            .position(|entry| entry.key == key)
            .ok_or(Error::Missing)?;
        self.registrations.remove(index);
        Ok(())
    }

    /// Delete every registration for a handle value, including its generation bits.
    pub fn close(&mut self, key: u32) {
        self.registrations.retain(|entry| entry.key != key);
    }

    /// True when at least one registration would be reported without changing edge state.
    ///
    /// The kernel uses this while holding a wait queue's lock immediately before enqueueing a
    /// waiter.  Keeping this query non-mutating means a source that wins that race is delivered by
    /// the subsequent [`Self::collect`] call rather than being consumed by the check itself.
    pub fn has_ready(&self, mut readiness: impl FnMut(u32) -> u32) -> bool {
        self.registrations.iter().any(|entry| {
            let current = readiness(entry.key) & entry.events;
            if entry.edge {
                current & !entry.seen != 0
            } else {
                current != 0
            }
        })
    }

    /// Observe readiness for all sources through a caller-provided query.
    ///
    /// Edge registrations emit newly-set bits once. Level registrations emit every time they are
    /// observed ready. The callback is intentionally read-only: the kernel holds its queue lock
    /// while taking a coherent readiness snapshot.
    #[allow(dead_code)] // Kernel delivery also needs source keys; host clients use the public view.
    pub fn collect(&mut self, max: usize, readiness: impl FnMut(u32) -> u32) -> Vec<Ready> {
        self.collect_sources(max, readiness)
            .into_iter()
            .map(|delivery| delivery.ready)
            .collect()
    }

    /// Observe readiness and retain each selected registration key for kernel-side acknowledgement.
    pub fn collect_sources(
        &mut self,
        max: usize,
        mut readiness: impl FnMut(u32) -> u32,
    ) -> Vec<Delivery> {
        let mut ready = Vec::new();
        for entry in &mut self.registrations {
            if ready.len() == max {
                break;
            }
            let current = readiness(entry.key) & entry.events;
            let report = if entry.edge {
                let changed = current & !entry.seen;
                entry.seen = current;
                changed
            } else {
                current
            };
            if report != 0 {
                ready.push(Delivery {
                    key: entry.key,
                    ready: Ready {
                        cookie: entry.cookie,
                        events: report,
                    },
                });
            }
        }
        ready
    }
}

fn validate(events: u32) -> Result<(u32, bool), Error> {
    let edge = events & EDGE != 0;
    let mask = events & !EDGE;
    if mask == 0 || mask & !EVENT_MASK != 0 {
        return Err(Error::InvalidMask);
    }
    Ok((mask, edge))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_collects_level() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE, 9), Ok(()));
        assert_eq!(
            queue.collect(8, |_| READABLE),
            [Ready {
                cookie: 9,
                events: READABLE
            }]
        );
    }

    #[test]
    fn level_repeats() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE, 1), Ok(()));
        assert_eq!(queue.collect(1, |_| READABLE).len(), 1);
        assert_eq!(queue.collect(1, |_| READABLE).len(), 1);
    }

    #[test]
    fn edge_emits_once() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE | EDGE, 1), Ok(()));
        assert_eq!(queue.collect(1, |_| READABLE).len(), 1);
        assert!(queue.collect(1, |_| READABLE).is_empty());
    }

    #[test]
    fn edge_rearms_after_clear() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE | EDGE, 1), Ok(()));
        let _ = queue.collect(1, |_| READABLE);
        let _ = queue.collect(1, |_| 0);
        assert_eq!(queue.collect(1, |_| READABLE).len(), 1);
    }

    #[test]
    fn edge_tracks_bits_independently() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE | HUP | EDGE, 1), Ok(()));
        assert_eq!(queue.collect(1, |_| READABLE)[0].events, READABLE);
        assert_eq!(queue.collect(1, |_| READABLE | HUP)[0].events, HUP);
    }

    #[test]
    fn delete_hides_cookie() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(4, SIGNALED, 99), Ok(()));
        assert_eq!(queue.delete(4), Ok(()));
        assert!(queue.collect(1, |_| SIGNALED).is_empty());
    }

    #[test]
    fn close_hides_cookie() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(4, SIGNALED, 99), Ok(()));
        queue.close(4);
        assert!(queue.collect(1, |_| SIGNALED).is_empty());
    }

    #[test]
    fn modify_replaces_cookie() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE, 1), Ok(()));
        assert_eq!(queue.modify(1, HUP, 2), Ok(()));
        assert_eq!(
            queue.collect(1, |_| READABLE | HUP)[0],
            Ready {
                cookie: 2,
                events: HUP
            }
        );
    }

    #[test]
    fn duplicate_is_rejected() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE, 1), Ok(()));
        assert_eq!(queue.add(1, READABLE, 1), Err(Error::Exists));
    }

    #[test]
    fn missing_modify_is_rejected() {
        assert_eq!(Queue::new().modify(1, READABLE, 1), Err(Error::Missing));
    }

    #[test]
    fn missing_delete_is_rejected() {
        assert_eq!(Queue::new().delete(1), Err(Error::Missing));
    }

    #[test]
    fn empty_mask_is_rejected() {
        assert_eq!(Queue::new().add(1, 0, 0), Err(Error::InvalidMask));
    }

    #[test]
    fn unknown_mask_is_rejected() {
        assert_eq!(Queue::new().add(1, 16, 0), Err(Error::InvalidMask));
    }

    #[test]
    fn max_limits_results() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE, 1), Ok(()));
        assert_eq!(queue.add(2, READABLE, 2), Ok(()));
        assert_eq!(queue.collect(1, |_| READABLE).len(), 1);
    }

    #[test]
    fn zero_max_is_empty() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE, 1), Ok(()));
        assert!(queue.collect(0, |_| READABLE).is_empty());
    }

    #[test]
    fn length_changes_with_operations() {
        let mut queue = Queue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.add(1, READABLE, 0), Ok(()));
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.delete(1), Ok(()));
        assert!(queue.is_empty());
    }

    #[test]
    fn full_queue_rejects_another_entry() {
        let mut queue = Queue::new();
        for key in 0..MAX_REGISTRATIONS as u32 {
            assert_eq!(queue.add(key, READABLE, 0), Ok(()));
        }
        assert_eq!(queue.add(u32::MAX, READABLE, 0), Err(Error::Full));
    }

    #[test]
    fn close_only_removes_matching_key() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE, 1), Ok(()));
        assert_eq!(queue.add(2, READABLE, 2), Ok(()));
        queue.close(1);
        assert_eq!(
            queue.collect(2, |_| READABLE),
            [Ready {
                cookie: 2,
                events: READABLE
            }]
        );
    }

    #[test]
    fn randomized_model_has_no_duplicate_cookie_after_delete() {
        let mut queue = Queue::new();
        let mut state = 1u64;
        for _ in 0..10_000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let key = (state >> 32) as u32 % 128;
            match state & 3 {
                0 => {
                    let _ = queue.add(
                        key,
                        READABLE | if state & 4 == 0 { EDGE } else { 0 },
                        u64::from(key),
                    );
                }
                1 => {
                    let _ = queue.modify(key, READABLE, u64::from(key));
                }
                2 => {
                    let _ = queue.delete(key);
                }
                _ => queue.close(key),
            }
            let events = queue.collect(128, |source| if source == key { READABLE } else { 0 });
            for event in events {
                assert_ne!(event.cookie, u64::MAX);
            }
        }
    }

    macro_rules! single_bit_filter {
        ($name:ident, $bit:expr) => {
            #[test]
            fn $name() {
                let mut queue = Queue::new();
                assert_eq!(queue.add(7, $bit, 3), Ok(()));
                assert!(queue.collect(1, |_| EVENT_MASK & !$bit).is_empty());
                assert_eq!(queue.collect(1, |_| $bit)[0].events, $bit);
            }
        };
    }

    single_bit_filter!(readable_filter, READABLE);
    single_bit_filter!(writable_filter, WRITABLE);
    single_bit_filter!(hup_filter, HUP);
    single_bit_filter!(signaled_filter, SIGNALED);

    #[test]
    fn edge_modify_rearms() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE | EDGE, 1), Ok(()));
        let _ = queue.collect(1, |_| READABLE);
        assert_eq!(queue.modify(1, READABLE | EDGE, 2), Ok(()));
        assert_eq!(queue.collect(1, |_| READABLE)[0].cookie, 2);
    }

    #[test]
    fn delete_then_add_accepts_same_key() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(3, HUP, 1), Ok(()));
        assert_eq!(queue.delete(3), Ok(()));
        assert_eq!(queue.add(3, SIGNALED, 2), Ok(()));
        assert_eq!(queue.collect(1, |_| SIGNALED)[0].cookie, 2);
    }

    #[test]
    fn level_mask_reports_all_ready_bits() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE | HUP, 1), Ok(()));
        assert_eq!(
            queue.collect(1, |_| READABLE | HUP)[0].events,
            READABLE | HUP
        );
    }

    #[test]
    fn edge_mask_reports_all_new_bits() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE | HUP | EDGE, 1), Ok(()));
        assert_eq!(
            queue.collect(1, |_| READABLE | HUP)[0].events,
            READABLE | HUP
        );
    }

    #[test]
    fn close_missing_key_is_harmless() {
        let mut queue = Queue::new();
        queue.close(9);
        assert!(queue.is_empty());
    }

    #[test]
    fn modify_preserves_table_length() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE, 1), Ok(()));
        assert_eq!(queue.modify(1, HUP, 2), Ok(()));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn edge_clear_does_not_emit() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, READABLE | EDGE, 1), Ok(()));
        assert!(queue.collect(1, |_| 0).is_empty());
    }

    #[test]
    fn has_ready_does_not_consume_an_edge() {
        let mut queue = Queue::new();
        assert_eq!(queue.add(1, SIGNALED | EDGE, 1), Ok(()));
        assert!(queue.has_ready(|_| SIGNALED));
        assert_eq!(queue.collect(1, |_| SIGNALED).len(), 1);
    }
}
