// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! A tolerant bottom-up merge sort for user comparators (`RD-new`).
//!
//! `core`'s `sort_by` may panic when a comparator is inconsistent (it assumes a total order for
//! its insertion-sort and merge bookkeeping). The C donors call `qsort`, whose contract is also
//! a total order, but a C callback that lies only produces an arbitrary order. This sort keeps
//! that behaviour: every index is bounds-checked, every loop advances a counter, and a
//! comparator that answers at random cannot panic, loop forever or read out of bounds.

use alloc::vec::Vec;

/// Sort `items` in place; allocation failure leaves the current order (the donor's `qsort`
/// fallback in `fts_sort`).
pub(crate) fn merge_sort<T: Clone>(items: &mut Vec<T>, cmp: &mut dyn FnMut(&T, &T) -> i32) {
    let len = items.len();
    if len < 2 {
        return;
    }
    let mut width = 1usize;
    let mut source = core::mem::take(items);
    let mut target: Vec<T> = Vec::new();
    if target.try_reserve(len).is_err() {
        *items = source;
        return;
    }
    while width < len {
        let mut start = 0usize;
        while start < len {
            let mid = start.wrapping_add(width).min(len);
            let end = start.wrapping_add(width.wrapping_mul(2)).min(len);
            let mut left = start;
            let mut right = mid;
            while left < mid && right < end {
                let take_left = match (source.get(left), source.get(right)) {
                    (Some(a), Some(b)) => cmp(a, b) <= 0,
                    _ => true,
                };
                if take_left {
                    if let Some(value) = source.get(left).cloned() {
                        target.push(value);
                    }
                    left = left.wrapping_add(1);
                } else {
                    if let Some(value) = source.get(right).cloned() {
                        target.push(value);
                    }
                    right = right.wrapping_add(1);
                }
            }
            while left < mid {
                if let Some(value) = source.get(left).cloned() {
                    target.push(value);
                }
                left = left.wrapping_add(1);
            }
            while right < end {
                if let Some(value) = source.get(right).cloned() {
                    target.push(value);
                }
                right = right.wrapping_add(1);
            }
            start = end;
        }
        core::mem::swap(&mut source, &mut target);
        target.clear();
        width = width.saturating_mul(2);
    }
    *items = source;
}

#[cfg(test)]
#[path = "sort_ut.rs"]
mod tests;
