// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Greedy case shrinking.
//!
//! A case's [`Case::shrink`] returns candidates in preference order. The
//! greedy loop takes the first candidate that still mismatches, repeats, and stops after a bound so
//! a badly written `shrink` cannot hang the suite.

use crate::diff::Case;

/// Greedily shrink `case` while `still_mismatches` holds, for at most `max_steps` steps.
pub fn greedy<C: Case>(
    case: &C,
    mut still_mismatches: impl FnMut(&C) -> bool,
    max_steps: usize,
) -> C {
    let mut current = case.clone();
    for _ in 0..max_steps {
        let Some(next) = current
            .shrink()
            .into_iter()
            .find(|candidate| still_mismatches(candidate))
        else {
            break;
        };
        current = next;
    }
    current
}

#[cfg(test)]
#[path = "shrink_ut.rs"]
mod tests;
