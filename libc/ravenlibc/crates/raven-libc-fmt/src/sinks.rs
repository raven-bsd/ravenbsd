// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Ready-made sinks and the glue buffer sink.
//!
//! `CountingSink` is the dry-run/`asprintf` counter; `SliceSink` is a simple fixed-buffer
//! collector for tests and callers; `BoundedSink` implements the `snprintf` rule "count
//! everything, store only what fits" (`donor-map/04` 5.2).

use core::marker::PhantomData;

use crate::traits::{CharUnit, Sink, SinkError};

/// A sink that counts output units and writes nothing.
#[derive(Clone, Copy)]
pub struct CountingSink<C> {
    count: u64,
    marker: PhantomData<fn() -> C>,
}

impl<C> CountingSink<C> {
    /// A counter at zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: 0,
            marker: PhantomData,
        }
    }

    /// The number of units written so far.
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }
}

impl<C> Default for CountingSink<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: CharUnit> Sink<C> for CountingSink<C> {
    fn write(&mut self, s: &[C]) -> Result<(), SinkError> {
        self.count = self.count.wrapping_add(s.len() as u64);
        Ok(())
    }

    fn pad(&mut self, _ch: C, n: usize) -> Result<(), SinkError> {
        self.count = self.count.wrapping_add(n as u64);
        Ok(())
    }
}

/// A sink that stores into a caller-provided slice and counts everything.
pub struct SliceSink<'a, C> {
    buf: &'a mut [C],
    len: usize,
    stored: usize,
}

impl<'a, C> SliceSink<'a, C> {
    /// A sink over `buf`.
    pub fn new(buf: &'a mut [C]) -> Self {
        Self {
            buf,
            len: 0,
            stored: 0,
        }
    }

    /// The total number of units written (including those that did not fit).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// The number of units actually stored.
    #[must_use]
    pub const fn stored(&self) -> usize {
        self.stored
    }

    /// Whether nothing was stored yet.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.stored == 0
    }
}

impl<C: CharUnit> Sink<C> for SliceSink<'_, C> {
    fn write(&mut self, s: &[C]) -> Result<(), SinkError> {
        let room = self.buf.len().saturating_sub(self.stored);
        let k = if s.len() < room { s.len() } else { room };
        if let (Some(dst), Some(src)) = (self.buf.get_mut(self.stored..), s.get(..k)) {
            dst.get_mut(..k).unwrap_or(&mut []).copy_from_slice(src);
        }
        self.stored = self.stored.saturating_add(k);
        self.len = self.len.wrapping_add(s.len());
        Ok(())
    }
}

/// A sink that counts every unit but stores at most `cap` of them.
///
/// `snprintf` passes `cap = n - 1`; the glue writes the terminating NUL afterwards.
pub struct BoundedSink<'a, C> {
    buf: &'a mut [C],
    cap: usize,
    len: usize,
    stored: usize,
}

impl<'a, C> BoundedSink<'a, C> {
    /// A bounded sink storing at most `cap` units into `buf`.
    pub fn new(buf: &'a mut [C], cap: usize) -> Self {
        Self {
            buf,
            cap,
            len: 0,
            stored: 0,
        }
    }

    /// The total number of units written (the value `snprintf` returns).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// The number of units actually stored.
    #[must_use]
    pub const fn stored(&self) -> usize {
        self.stored
    }

    /// Whether nothing was stored yet.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.stored == 0
    }
}

impl<C: CharUnit> Sink<C> for BoundedSink<'_, C> {
    fn write(&mut self, s: &[C]) -> Result<(), SinkError> {
        let room = self.cap.min(self.buf.len()).saturating_sub(self.stored);
        let k = if s.len() < room { s.len() } else { room };
        if let (Some(dst), Some(src)) = (self.buf.get_mut(self.stored..), s.get(..k)) {
            dst.get_mut(..k).unwrap_or(&mut []).copy_from_slice(src);
        }
        self.stored = self.stored.saturating_add(k);
        self.len = self.len.wrapping_add(s.len());
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "sinks_ut.rs"]
mod tests;
