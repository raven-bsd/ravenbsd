// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The sampling decision.

use crate::TraceId;

/// A trace sampling policy, decided once per trace and inherited by every span.
///
/// [`Sampling::decide`] is a pure function of the [`TraceId`], so all spans of one trace — which
/// share the id — observe the same decision without any shared state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sampling {
    /// Never sample; ids still propagate but no span is recorded for export.
    Never,
    /// Always sample.
    Always,
    /// Sample one trace in `n`; `n == 0` behaves like [`Sampling::Never`].
    Every(u64),
}

impl Sampling {
    /// Whether a trace with `id` is sampled.
    pub fn decide(self, id: TraceId) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::Every(n) => n != 0 && id.raw().is_multiple_of(n),
        }
    }
}

#[cfg(test)]
#[path = "sampling_ut.rs"]
mod tests;
