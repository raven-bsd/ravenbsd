// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 2016-2020 Netflix, Inc.
//
// Portions derived from FreeBSD `sys/netinet/tcp_stacks/rack.c` and
// `sys/netinet/tcp_hpts.c`/`tcp_hpts.h` at
// 133344405f532bd54ff3629a77d4c4e062f01fdb (BSD-2-Clause). This sans-IO
// model is distributed under the project BSD-3-Clause licence; the donor
// BSD-2 notice is retained verbatim below.
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

//! Sans-IO FreeBSD HPTS/timing-wheel context for `tcp_stacks/rack.c`.
//!
//! FreeBSD `sys/netinet/tcp_hpts.h`/`tcp_hpts.c` define a 10-microsecond
//! timing wheel; `tcp_stacks/rack.c` uses it for
//! `rack_timer_start`/`rack_timer_cancel`/`rack_remxt_tmr`/`rack_stopall`
//! and the `rack_timeout_*` actions. This context models the schedule,
//! per-entry timer flags, slot/FIFO callback order, now-ms and callback
//! actions without kernel locks or stack wiring.
//!
//! The wheel is sparse (absolute 10-µs slots in a `BTreeMap`) rather than
//! the donor's fixed 102400-slot ring. Slot ordering, FIFO order within a
//! slot, and the 10-µs conversion are preserved; the fixed ring wrap and
//! the HPTS CPU fan-out are K269 integration details.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

/// `HPTS_USECS_PER_SLOT` from `tcp_hpts.h`.
pub const HPTS_USECS_PER_SLOT: u64 = 10;

/// `NUM_OF_HPTSI_SLOTS` from `tcp_hpts_internal.h`.
pub const NUM_OF_HPTSI_SLOTS: usize = 102_400;

/// `PACE_TMR_DELACK`.
pub const PACE_TMR_DELACK: u8 = 0x01;
/// `PACE_TMR_RACK`.
pub const PACE_TMR_RACK: u8 = 0x02;
/// `PACE_TMR_TLP`.
pub const PACE_TMR_TLP: u8 = 0x04;
/// `PACE_TMR_RXT`.
pub const PACE_TMR_RXT: u8 = 0x08;
/// `PACE_TMR_PERSIT`.
pub const PACE_TMR_PERSIT: u8 = 0x10;
/// `PACE_TMR_KEEP`.
pub const PACE_TMR_KEEP: u8 = 0x20;
/// `PACE_TMR_MASK`.
pub const PACE_TMR_MASK: u8 =
    PACE_TMR_KEEP | PACE_TMR_PERSIT | PACE_TMR_RXT | PACE_TMR_TLP | PACE_TMR_RACK | PACE_TMR_DELACK;

/// `HPTS_USEC_TO_SLOTS`: round usecs up to 10-µs slots.
#[must_use]
pub const fn hpts_usec_to_slots(usecs: u64) -> u64 {
    usecs.saturating_add(HPTS_USECS_PER_SLOT - 1) / HPTS_USECS_PER_SLOT
}

/// The `rack_timeout_*` action selected by the donor dispatch order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackTimeout {
    /// `rack_timeout_delack`.
    Delack,
    /// `rack_timeout_rack`.
    Rack,
    /// `rack_timeout_tlp`.
    Tlp,
    /// `rack_timeout_rxt`.
    Rxt,
    /// `rack_timeout_persist`.
    Persist,
    /// `rack_timeout_keepalive`.
    Keepalive,
}

/// One callback returned by [`RackHpts::tcp_hpts_softclock`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HptsCallback {
    /// Connection/entry identifier.
    pub id: u32,
    /// `rc_hpts_flags` timer bits that fired.
    pub timers: u8,
    /// Callback time in microseconds.
    pub now_us: u64,
    /// Callback time in milliseconds.
    pub now_ms: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct HptsEntry {
    timers: u8,
    rc_tmr_stopped: u8,
    generation: u64,
    timers_stopped: bool,
    timer_override: bool,
    in_persist: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HptsRef {
    id: u32,
    generation: u64,
}

/// A sans-IO HPTS context with one timer/queue entry per connection id.
#[derive(Clone, Debug)]
pub struct RackHpts {
    now_us: u64,
    wheel: BTreeMap<u64, VecDeque<HptsRef>>,
    entries: BTreeMap<u32, HptsEntry>,
}

impl RackHpts {
    /// Create an empty context at `now_us`.
    #[must_use]
    pub const fn new(now_us: u64) -> Self {
        Self {
            now_us,
            wheel: BTreeMap::new(),
            entries: BTreeMap::new(),
        }
    }

    /// Current time in microseconds.
    #[must_use]
    pub const fn now_us(&self) -> u64 {
        self.now_us
    }

    /// Current time in milliseconds.
    #[must_use]
    pub const fn now_ms(&self) -> u64 {
        self.now_us / 1_000
    }

    /// `tcp_hpts_init`: create the per-connection HPTS entry.
    pub fn tcp_hpts_init(&mut self, id: u32) {
        self.entries.entry(id).or_default();
    }

    /// `tcp_hpts_insert`: queue the connection in the slot `usecs` from now.
    ///
    /// Returns the absolute due time in microseconds. This follows the donor
    /// `__tcp_hpts_insert` queueing step; timer flags are owned by
    /// [`Self::rack_timer_start`].
    pub fn tcp_hpts_insert(&mut self, id: u32, usecs: u64) -> u64 {
        self.arm(id, usecs)
    }

    /// Arm the wheel for `id`, invalidating any older queue reference.
    fn arm(&mut self, id: u32, usecs: u64) -> u64 {
        let entry = self.entries.entry(id).or_default();
        entry.generation = entry.generation.wrapping_add(1);
        let generation = entry.generation;
        let slots = hpts_usec_to_slots(usecs);
        let due_slot = self
            .now_us
            .saturating_div(HPTS_USECS_PER_SLOT)
            .saturating_add(slots);
        self.wheel
            .entry(due_slot)
            .or_default()
            .push_back(HptsRef { id, generation });
        due_slot.saturating_mul(HPTS_USECS_PER_SLOT)
    }

    /// `tcp_hpts_remove`: clear the timer flags and invalidate queued slots.
    ///
    /// Returns the flags that were pending. `rc_tmr_stopped` retains them for
    /// the next `rack_start_hpts_timer` accounting pass, as in the donor.
    pub fn tcp_hpts_remove(&mut self, id: u32) -> u8 {
        let Some(entry) = self.entries.get_mut(&id) else {
            return 0;
        };
        let timers = entry.timers;
        entry.timers = 0;
        entry.rc_tmr_stopped = timers & PACE_TMR_MASK;
        entry.generation = entry.generation.wrapping_add(1);
        timers
    }

    /// `tcp_hpts_softclock`: run due callbacks in slot then FIFO order.
    ///
    /// A queued connection with no timer flags is still delivered: the donor
    /// uses those HPTS wakeups for output pacing and input processing.
    pub fn tcp_hpts_softclock(&mut self, now_us: u64) -> Vec<HptsCallback> {
        self.now_us = now_us;
        let target = now_us / HPTS_USECS_PER_SLOT;
        let keys: Vec<u64> = self.wheel.range(..=target).map(|(slot, _)| *slot).collect();
        let mut callbacks = Vec::new();
        for key in keys {
            let Some(refs) = self.wheel.remove(&key) else {
                continue;
            };
            for reference in refs {
                let Some(entry) = self.entries.get_mut(&reference.id) else {
                    continue;
                };
                if entry.generation != reference.generation {
                    continue;
                }
                let timers = entry.timers;
                entry.timers = 0;
                entry.rc_tmr_stopped = 0;
                entry.generation = entry.generation.wrapping_add(1);
                callbacks.push(HptsCallback {
                    id: reference.id,
                    timers,
                    now_us,
                    now_ms: now_us / 1_000,
                });
            }
        }
        callbacks
    }

    /// `rack_timer_start`: replace the timer flags and arm the HPTS slot.
    ///
    /// `rack_start_hpts_timer` resets `rc_hpts_flags` before calling the donor
    /// `rack_timer_start`, so a new start replaces the previous timer. Persist
    /// and stop-all entries refuse a start. Returns the absolute due time in
    /// microseconds; a refused start returns 0.
    pub fn rack_timer_start(&mut self, id: u32, timers: u8, usecs: u64) -> u64 {
        {
            let entry = self.entries.entry(id).or_default();
            if entry.timers_stopped || entry.in_persist {
                return 0;
            }
            entry.timers = timers;
            entry.rc_tmr_stopped = 0;
        }
        self.arm(id, usecs.max(1))
    }

    /// `rack_timer_cancel`: clear every timer bit and invalidate the slot.
    pub fn rack_timer_cancel(&mut self, id: u32) -> u8 {
        self.tcp_hpts_remove(id)
    }

    /// `rack_remxt_tmr`: cancel the timer and mark a retransmit override.
    pub fn rack_remxt_tmr(&mut self, id: u32) -> u8 {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.timer_override = true;
        } else {
            self.entries.insert(
                id,
                HptsEntry {
                    timer_override: true,
                    ..HptsEntry::default()
                },
            );
        }
        self.tcp_hpts_remove(id)
    }

    /// `rack_stopall`: stop all timers and refusal future scheduling.
    pub fn rack_stopall(&mut self, id: u32) -> u8 {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.timers_stopped = true;
        } else {
            self.entries.insert(
                id,
                HptsEntry {
                    timers_stopped: true,
                    ..HptsEntry::default()
                },
            );
        }
        self.tcp_hpts_remove(id)
    }

    /// `rack_stop_all_timers`: remove timers and remember a pending persist.
    pub fn rack_stop_all_timers(&mut self, id: u32) -> u8 {
        let timers = self.entries.get(&id).map_or(0, |entry| entry.timers);
        if timers & PACE_TMR_PERSIT != 0
            && let Some(entry) = self.entries.get_mut(&id)
        {
            entry.in_persist = true;
        }
        self.tcp_hpts_remove(id)
    }
}

/// The `rack_timeout_*` dispatch order.
#[must_use]
pub const fn rack_timeout_action(timers: u8) -> Option<RackTimeout> {
    if timers & PACE_TMR_DELACK != 0 {
        Some(RackTimeout::Delack)
    } else if timers & PACE_TMR_RACK != 0 {
        Some(RackTimeout::Rack)
    } else if timers & PACE_TMR_TLP != 0 {
        Some(RackTimeout::Tlp)
    } else if timers & PACE_TMR_RXT != 0 {
        Some(RackTimeout::Rxt)
    } else if timers & PACE_TMR_PERSIT != 0 {
        Some(RackTimeout::Persist)
    } else if timers & PACE_TMR_KEEP != 0 {
        Some(RackTimeout::Keepalive)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "rack_hpts_ut.rs"]
mod tests;
