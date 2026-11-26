// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent range reference for SACK/DSACK/reneging.
//!
//! The reference keeps only the observable range set and DSACK counters. It is deliberately
//! simpler than the product sendmap so a product regression in split/merge/trim ordering is
//! visible in the matrix comparison.

use std::vec::Vec;

/// A minimal independent SACK range reference.
#[derive(Clone, Debug, Default)]
pub struct ReferenceSack {
    /// Sacked ranges, sorted and merged.
    pub ranges: Vec<(u32, u32)>,
    /// A DSACK round has begun.
    pub dsack_round_seen: bool,
    /// DSACK bytes after the last recovery.
    pub dsack_byte_cnt: u32,
    /// DSACK rounds.
    pub num_dsacks: u32,
    /// DSACK persist budget.
    pub dsack_persist: u8,
    /// A spurious-recovery signal is pending.
    pub might_revert: bool,
    /// Retransmitted bytes during recovery.
    pub retran_during_recovery: u32,
    /// In fast recovery.
    pub in_fast_recovery: bool,
}

impl ReferenceSack {
    /// Create an empty reference.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total sacked bytes.
    #[must_use]
    pub fn sacked_bytes(&self) -> u32 {
        self.ranges
            .iter()
            .fold(0u32, |sum, (start, end)| sum.saturating_add(end - start))
    }

    /// Add a SACK block and return the newly covered bytes.
    pub fn sack_block(&mut self, start: u32, end: u32) -> u32 {
        if start >= end {
            return 0;
        }
        let before = self.sacked_bytes();
        self.ranges.push((start, end));
        self.ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (s, e) in std::mem::take(&mut self.ranges) {
            if let Some((_, last_end)) = merged.last_mut()
                && s <= *last_end
            {
                if e > *last_end {
                    *last_end = e;
                }
            } else {
                merged.push((s, e));
            }
        }
        self.ranges = merged;
        self.sacked_bytes().saturating_sub(before)
    }

    /// Remove/trim ranges below `ack`, returning the removed bytes.
    pub fn trim_below(&mut self, ack: u32) -> u32 {
        let mut removed = 0u32;
        let mut kept = Vec::new();
        for (start, end) in std::mem::take(&mut self.ranges) {
            if end <= ack {
                removed = removed.saturating_add(end - start);
            } else if start < ack {
                removed = removed.saturating_add(ack - start);
                kept.push((ack, end));
            } else {
                kept.push((start, end));
            }
        }
        self.ranges = kept;
        removed
    }

    /// DSACK accounting without TLP exclusion (the random matrix sets no TLP markers).
    pub fn note_dsack(&mut self, start: u32, end: u32) {
        let amount = end.saturating_sub(start);
        if !self.dsack_round_seen {
            self.dsack_round_seen = true;
            self.num_dsacks = self.num_dsacks.saturating_add(1);
            self.dsack_persist = 16;
        }
        self.dsack_byte_cnt = self.dsack_byte_cnt.saturating_add(amount);
        if !self.in_fast_recovery
            && self.retran_during_recovery != 0
            && self.dsack_byte_cnt >= self.retran_during_recovery
        {
            self.might_revert = true;
            self.retran_during_recovery = 0;
            self.dsack_byte_cnt = 0;
        }
    }
}
