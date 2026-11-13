// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Event queues, timers and user events of the fake kernel.
//!
//! Mirrors `kernel/src/syscall/event.rs:47-289`: `EVQ_CREATE` accepts flags 0, `EVQ_CTL` uses
//! ADD=1/MOD=2/DEL=3, `TIMER_CREATE` accepts only `CLOCK_MONOTONIC` with flags 0, and
//! `EVENT_SIGNAL`/`TIMER_SET` require the object's rights.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

/// `READABLE`.
pub const READABLE: u32 = 1;
/// `WRITABLE`.
pub const WRITABLE: u32 = 2;
/// `HUP`.
pub const HUP: u32 = 4;
/// `SIGNALED` (timer expired, event signalled, IRQ fired).
pub const SIGNALED: u32 = 8;
/// Edge-triggered bit.
pub const EDGE: u32 = 0x8000_0000;
/// Add a registration.
pub const ADD: u32 = 1;
/// Modify a registration.
pub const MOD: u32 = 2;
/// Delete a registration.
pub const DEL: u32 = 3;

/// Valid event-mask bits.
pub const MASK: u32 = READABLE | WRITABLE | HUP | SIGNALED | EDGE;

/// One event-queue registration.
#[derive(Clone, Copy, Debug)]
pub struct Registration {
    /// Registered handle.
    pub handle: Handle,
    /// Requested event mask.
    pub events: u32,
    /// Caller cookie.
    pub cookie: u64,
}

/// One event queue.
#[derive(Clone, Debug, Default)]
pub struct EventQueue {
    /// Registrations in insertion order.
    pub registrations: host::vec::Vec<Registration>,
}

/// One timer object.
#[derive(Clone, Debug)]
pub struct Timer {
    /// `CLOCK_MONOTONIC`.
    pub clock_id: u32,
    /// Absolute monotonic deadline in nanoseconds; 0 means disarmed.
    pub deadline_ns: u64,
    /// Repetition interval; 0 means one-shot.
    pub interval_ns: u64,
    /// True while armed.
    pub armed: bool,
}

impl Timer {
    /// A disarmed timer for `clock_id`.
    pub fn new(clock_id: u32) -> Self {
        Self {
            clock_id,
            deadline_ns: 0,
            interval_ns: 0,
            armed: false,
        }
    }

    /// Arm (`initial_ns != 0`) or disarm the timer.
    pub fn set(&mut self, now_ns: u64, initial_ns: u64, interval_ns: u64) {
        if initial_ns == 0 {
            self.armed = false;
            self.deadline_ns = 0;
            self.interval_ns = 0;
            return;
        }
        self.armed = true;
        self.deadline_ns = now_ns.saturating_add(initial_ns);
        self.interval_ns = interval_ns;
    }

    /// True when the deadline has passed.
    pub fn ready(&self, now_ns: u64) -> bool {
        self.armed && now_ns >= self.deadline_ns
    }

    /// Consume one expiry and rearm a repeating timer.
    pub fn consume(&mut self, now_ns: u64) {
        if self.interval_ns == 0 {
            self.armed = false;
            self.deadline_ns = 0;
        } else {
            self.deadline_ns = now_ns.saturating_add(self.interval_ns);
        }
    }
}

/// One user event (doorbell).
#[derive(Clone, Debug, Default)]
pub struct UserEvent {
    /// Last signalled value.
    pub value: u64,
    /// Number of signals not yet consumed by a wait.
    pub pending: u32,
}

impl UserEvent {
    /// Set the signalled value and increment the pending count.
    pub fn signal(&mut self, value: u64) {
        self.value = value;
        self.pending = self.pending.saturating_add(1);
    }

    /// Consume one pending signal.
    pub fn consume(&mut self) -> bool {
        if self.pending == 0 {
            return false;
        }
        self.pending -= 1;
        true
    }
}

/// The event world.
#[derive(Clone, Debug, Default)]
pub struct EventWorld {
    /// Event queues by id.
    pub queues: host::vec::Vec<EventQueue>,
    /// Timers by id.
    pub timers: host::vec::Vec<Timer>,
    /// User events by id.
    pub events: host::vec::Vec<UserEvent>,
}

impl EventWorld {
    /// Look up a queue.
    pub fn queue(&self, id: usize) -> Result<&EventQueue, Errno> {
        self.queues.get(id).ok_or(Errno::EBADF)
    }

    /// Look up a queue mutably.
    pub fn queue_mut(&mut self, id: usize) -> Result<&mut EventQueue, Errno> {
        self.queues.get_mut(id).ok_or(Errno::EBADF)
    }

    /// Look up a timer mutably.
    pub fn timer_mut(&mut self, id: usize) -> Result<&mut Timer, Errno> {
        self.timers.get_mut(id).ok_or(Errno::EBADF)
    }

    /// Look up an event mutably.
    pub fn event_mut(&mut self, id: usize) -> Result<&mut UserEvent, Errno> {
        self.events.get_mut(id).ok_or(Errno::EBADF)
    }
}
