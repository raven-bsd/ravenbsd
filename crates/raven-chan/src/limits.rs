// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Queue and in-flight bounds for one channel.

/// Default number of calls an endpoint may queue for its peer.
pub const DEFAULT_RECEIVE_QUEUE: usize = 64;

/// Default number of accepted calls an endpoint may leave unreaped.
pub const DEFAULT_IN_FLIGHT: usize = 64;

/// Largest accepted value for either limit.
///
/// The cap keeps a mistaken configuration from allocating an unbounded queue; the kernel
/// integration passes its own, smaller policy value.
pub const MAX_CAPACITY: usize = 1024;

/// Bounds for one channel.
///
/// [`Channel::with_limits`](crate::Channel::with_limits) clamps both fields to `1..=MAX_CAPACITY`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Limits {
    /// Maximum calls queued at an endpoint that it has not received yet.
    pub receive_queue: usize,
    /// Maximum accepted calls the caller has not reaped yet.
    ///
    /// An outstanding call and an unread completion both count. This is the bound that stops a
    /// caller from growing the completion queue without limit.
    pub in_flight: usize,
}

impl Limits {
    /// Create a limit set; values are clamped when the channel is built.
    pub const fn new(receive_queue: usize, in_flight: usize) -> Self {
        Self {
            receive_queue,
            in_flight,
        }
    }

    /// Clamp both fields into the supported range.
    pub(crate) const fn clamped(self) -> Self {
        Self {
            receive_queue: clamp_capacity(self.receive_queue),
            in_flight: clamp_capacity(self.in_flight),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            receive_queue: DEFAULT_RECEIVE_QUEUE,
            in_flight: DEFAULT_IN_FLIGHT,
        }
    }
}

const fn clamp_capacity(value: usize) -> usize {
    if value == 0 {
        1
    } else if value > MAX_CAPACITY {
        MAX_CAPACITY
    } else {
        value
    }
}

#[cfg(test)]
#[path = "limits_ut.rs"]
mod tests;
