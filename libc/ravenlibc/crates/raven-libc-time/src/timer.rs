// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: CLEANROOM POSIX timer state machine from
// architecture/raven-libc/donor-map/06-time-tz.md sections 5.3 and 6.4. The
// illumos `port/rt` files were read for behaviour only under DR-40; no
// expression from them is reproduced here.

//! The pure POSIX-timer state machine.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: all timer deadlines use checked/saturating helpers.
//!
//! The kernel's `TIMER_SET` takes a relative `initial_ns`; this table keeps the
//! armed time so `timer_gettime` can answer while PG-19 (`TIMER_GET`) is absent.
//! An `EVQ` cookie is the [`TimerId`]; a stale cookie from a deleted slot is
//! ignored because the generation no longer matches.

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::errno::{EAGAIN, EINVAL};
use raven_libc_abi::structs::Timespec;

use crate::clock::clock_id;
use crate::sys::{ClockId, errno};

/// A timer handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerId(pub u64);

/// One timer's interval and next value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ITimerSpec {
    /// Repeating interval.
    pub interval: Timespec,
    /// Time to the next expiration.
    pub value: Timespec,
}

impl Default for ITimerSpec {
    fn default() -> Self {
        Self {
            interval: Timespec::ZERO,
            value: Timespec::ZERO,
        }
    }
}

/// How an expiring timer notifies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notify {
    /// Raise a signal with a value.
    Signal {
        /// Signal number.
        signo: i32,
        /// `sigev_value`.
        value: i64,
    },
    /// Start the notification function on a thread.
    Thread,
    /// Update overrun state only.
    None,
}

/// A kernel operation the shell must perform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelOp {
    /// Create a timer for `clock`.
    Create {
        /// Clock the timer follows.
        clock: ClockId,
    },
    /// Arm (`initial_ns != 0`) or disarm (`initial_ns == 0`).
    Set {
        /// Relative initial delay in nanoseconds.
        initial_ns: u64,
        /// Repeating interval in nanoseconds.
        interval_ns: u64,
    },
    /// Close the timer handle.
    Close,
}

/// An expiry action for the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Raise `signo` with `value`.
    Signal {
        /// Signal number.
        signo: i32,
        /// Signal value.
        value: i64,
    },
    /// Start the notification thread.
    StartThread,
    /// No external action.
    Nothing,
}

/// The clock and monotonic time passed to a timer operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Now {
    /// The timer clock's current time.
    pub clock: Timespec,
    /// The monotonic clock in nanoseconds.
    pub mono_ns: u64,
}

#[derive(Clone, Copy, Debug)]
struct Timer {
    notify: Notify,
    first_deadline: u64,
    interval_ns: u64,
    armed: bool,
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    generation: u32,
    timer: Option<Timer>,
}

impl Slot {
    const fn empty() -> Self {
        Self {
            generation: 0,
            timer: None,
        }
    }
}

/// A bounded table of timers.
#[derive(Debug)]
pub struct TimerTable {
    slots: Vec<Slot>,
    max: usize,
}

impl TimerTable {
    /// Create a table with at most `max` live timers.
    #[must_use]
    pub fn new(max: usize) -> Self {
        Self {
            slots: Vec::new(),
            max,
        }
    }

    /// Number of live timers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.timer.is_some())
            .count()
    }

    /// Whether the table has no live timers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Create a timer and return its id and the kernel creation op.
    pub fn create(&mut self, clock: i32, notify: Notify) -> Result<(TimerId, KernelOp), Errno> {
        let Some(clock) = clock_id(clock) else {
            return Err(errno(EINVAL));
        };
        if let Notify::Signal { signo, .. } = notify
            && !(1..=64).contains(&signo)
        {
            return Err(errno(EINVAL));
        }
        let mut index = self.slots.iter().position(|slot| slot.timer.is_none());
        if index.is_none() {
            if self.slots.len() >= self.max {
                return Err(errno(EAGAIN));
            }
            self.slots.push(Slot::empty());
            index = Some(self.slots.len() - 1);
        }
        let index = index.ok_or(errno(EAGAIN))?;
        let slot = self.slots.get_mut(index).ok_or(errno(EAGAIN))?;
        slot.generation = slot.generation.wrapping_add(1).max(1);
        slot.timer = Some(Timer {
            notify,
            first_deadline: 0,
            interval_ns: 0,
            armed: false,
        });
        let id = TimerId((u64::from(slot.generation) << 32) | index as u64);
        Ok((id, KernelOp::Create { clock }))
    }

    /// Arm or disarm a timer; return the previous value and the kernel op.
    pub fn settime(
        &mut self,
        id: TimerId,
        absolute: bool,
        new: ITimerSpec,
        now: Now,
    ) -> Result<(ITimerSpec, KernelOp), Errno> {
        validate_spec(&new.value)?;
        validate_spec(&new.interval)?;
        let old = self.gettime(id, now.mono_ns)?;
        let index = self.index(id).ok_or(errno(EINVAL))?;
        let slot = self.slots.get_mut(index).ok_or(errno(EINVAL))?;
        let timer = slot.timer.as_mut().ok_or(errno(EINVAL))?;
        let value_ns = to_ns(&new.value);
        let interval_ns = to_ns(&new.interval);
        if value_ns == 0 {
            timer.armed = false;
            timer.interval_ns = 0;
            return Ok((
                old,
                KernelOp::Set {
                    initial_ns: 0,
                    interval_ns: 0,
                },
            ));
        }
        let initial_ns = if absolute {
            let now_ns = to_ns(&now.clock);
            value_ns.saturating_sub(now_ns).max(1)
        } else {
            value_ns.max(1)
        };
        timer.armed = true;
        timer.first_deadline = now.mono_ns.saturating_add(initial_ns);
        timer.interval_ns = interval_ns;
        Ok((
            old,
            KernelOp::Set {
                initial_ns,
                interval_ns,
            },
        ))
    }

    /// Read the value that would be returned by `timer_gettime`.
    pub fn gettime(&self, id: TimerId, now_mono_ns: u64) -> Result<ITimerSpec, Errno> {
        let index = self.index(id).ok_or(errno(EINVAL))?;
        let timer = self
            .slots
            .get(index)
            .and_then(|slot| slot.timer.as_ref())
            .ok_or(errno(EINVAL))?;
        if !timer.armed {
            return Ok(ITimerSpec::default());
        }
        let value_ns = if timer.interval_ns == 0 {
            timer.first_deadline.saturating_sub(now_mono_ns)
        } else if now_mono_ns < timer.first_deadline {
            timer.first_deadline - now_mono_ns
        } else {
            let elapsed = now_mono_ns - timer.first_deadline;
            let remainder = elapsed % timer.interval_ns;
            if remainder == 0 {
                timer.interval_ns
            } else {
                timer.interval_ns - remainder
            }
        };
        Ok(ITimerSpec {
            interval: from_ns(timer.interval_ns),
            value: from_ns(value_ns),
        })
    }

    /// The overrun count; PG-19 has no `TIMER_GET`, so this is always zero.
    #[must_use]
    pub const fn getoverrun(&self, _id: TimerId) -> u32 {
        0
    }

    /// Delete a timer and return the close operation.
    pub fn delete(&mut self, id: TimerId) -> Result<KernelOp, Errno> {
        let index = self.index(id).ok_or(errno(EINVAL))?;
        let slot = self.slots.get_mut(index).ok_or(errno(EINVAL))?;
        if slot.timer.take().is_none() {
            return Err(errno(EINVAL));
        }
        slot.generation = slot.generation.wrapping_add(1).max(1);
        Ok(KernelOp::Close)
    }

    /// Handle one expiry cookie.
    pub fn on_event(&mut self, cookie: u64, now_mono_ns: u64) -> Option<Action> {
        let id = TimerId(cookie);
        let index = self.index(id)?;
        let slot = self.slots.get_mut(index)?;
        let timer = slot.timer.as_mut()?;
        if !timer.armed {
            return None;
        }
        if timer.interval_ns == 0 {
            timer.armed = false;
        } else {
            let mut next = timer.first_deadline.saturating_add(timer.interval_ns);
            while next <= now_mono_ns {
                next = next.saturating_add(timer.interval_ns);
            }
            timer.first_deadline = next;
        }
        Some(match timer.notify {
            Notify::Signal { signo, value } => Action::Signal { signo, value },
            Notify::Thread => Action::StartThread,
            Notify::None => Action::Nothing,
        })
    }

    /// Forget every timer after `fork`.
    pub fn reset_after_fork(&mut self) {
        self.slots.clear();
    }

    fn index(&self, id: TimerId) -> Option<usize> {
        let index = usize::try_from(id.0 & 0xffff_ffff).ok()?;
        let generation = u32::try_from(id.0 >> 32).ok()?;
        let slot = self.slots.get(index)?;
        (slot.generation == generation && slot.timer.is_some()).then_some(index)
    }
}

fn validate_spec(spec: &Timespec) -> Result<(), Errno> {
    if spec.tv_sec < 0 || !(0..1_000_000_000).contains(&spec.tv_nsec) {
        return Err(errno(EINVAL));
    }
    Ok(())
}

fn to_ns(spec: &Timespec) -> u64 {
    if spec.tv_sec < 0 {
        return 0;
    }
    (spec.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(spec.tv_nsec as u64)
}

fn from_ns(ns: u64) -> Timespec {
    Timespec {
        tv_sec: (ns / 1_000_000_000) as i64,
        tv_nsec: (ns % 1_000_000_000) as i64,
    }
}

#[cfg(test)]
#[path = "timer_ut.rs"]
mod tests;
