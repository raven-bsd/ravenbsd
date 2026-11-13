// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mbstate_t` as two explicit words.

/// The C `mbstate_t`: two 32-bit words, size 8, align 4 (doc 14 3.A.2).
///
/// Word 0 is the musl UTF-8 partial-character state, shared with `mbrtoc16` and `mbrtoc32`
/// (a pending low surrogate is positive, a partial UTF-8 character has the high bit set).
/// Word 1 holds the `mbrtoc8` re-layout: `nleft` in the low byte and three pending bytes above it.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct MbState {
    /// First state word (UTF-8 decoder, pending surrogate).
    pub opaque1: u32,
    /// Second state word (`mbrtoc8` pending continuation bytes).
    pub opaque2: u32,
}

impl MbState {
    /// The initial conversion state.
    pub const INITIAL: MbState = MbState {
        opaque1: 0,
        opaque2: 0,
    };

    /// Whether both words are zero.
    #[must_use]
    pub const fn is_initial(&self) -> bool {
        self.opaque1 == 0 && self.opaque2 == 0
    }
}

#[cfg(test)]
#[path = "state_ut.rs"]
mod tests;
