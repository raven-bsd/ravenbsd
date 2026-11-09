// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Frame-pointer backtrace helper.
//!
//! The walker takes a reader closure and a validity predicate so it can be used on the exception
//! path without dereferencing an unchecked pointer. It never allocates and stops after 32 frames.

/// Maximum number of frames visited.
pub const MAX_FRAMES: usize = 32;

/// Walk the `rbp` chain starting at `start`.
///
/// `read` returns the 8-byte value at an address or `None` if it cannot be read.
/// `valid` says whether an address is inside a readable range. `visit` receives
/// `(depth, frame_pointer, return_address)` for each valid frame. Returns the number of frames.
pub fn walk<F, V, D>(start: u64, mut read: F, mut valid: V, mut visit: D) -> usize
where
    F: FnMut(u64) -> Option<u64>,
    V: FnMut(u64) -> bool,
    D: FnMut(usize, u64, u64),
{
    let mut rbp = start;
    let mut depth = 0usize;
    while depth < MAX_FRAMES && rbp != 0 {
        if !rbp.is_multiple_of(8) || !valid(rbp) {
            break;
        }
        let Some(previous) = read(rbp) else {
            break;
        };
        let Some(return_address) = rbp.checked_add(8).and_then(&mut read) else {
            break;
        };
        if return_address == 0 {
            break;
        }
        visit(depth, rbp, return_address);
        depth = depth.saturating_add(1);
        if previous == rbp {
            break;
        }
        rbp = previous;
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::walk;

    fn chain(frames: &[(u64, u64)]) -> impl FnMut(u64) -> Option<u64> + '_ {
        move |address| {
            for &(rbp, return_address) in frames {
                if address == rbp {
                    return Some(return_address);
                }
                if address == rbp.saturating_add(8) {
                    return Some(return_address);
                }
            }
            None
        }
    }

    #[test]
    fn empty_chain() {
        let keys: [u64; 0] = [];
        let read = chain(&[(0x1000, 0x2000)]);
        let mut visited = 0;
        assert_eq!(
            walk(
                0,
                read,
                |value| keys.contains(&value),
                |_, _, _| visited += 1
            ),
            0
        );
        assert_eq!(visited, 0);
    }

    #[test]
    fn single_frame() {
        let read = chain(&[(0x1000, 0x2000)]);
        let mut frames = 0;
        let count = walk(0x1000, read, |_| true, |_, _, _| frames += 1);
        assert_eq!(count, 1);
        assert_eq!(frames, 1);
    }

    #[test]
    fn two_frames() {
        let read = chain(&[(0x1000, 0x2000), (0x2000, 0x3000)]);
        let mut last = 0;
        let count = walk(
            0x1000,
            read,
            |_| true,
            |depth, _, rip| last = depth + rip as usize,
        );
        assert_eq!(count, 2);
        assert!(last > 0);
    }

    #[test]
    fn misaligned_frame_stops() {
        let read = chain(&[(0x1001, 0x2000)]);
        assert_eq!(walk(0x1001, read, |_| true, |_, _, _| {}), 0);
    }

    #[test]
    fn invalid_range_stops() {
        let read = chain(&[(0x1000, 0x2000)]);
        assert_eq!(
            walk(0x1000, read, |address| address != 0x1000, |_, _, _| {}),
            0
        );
    }

    #[test]
    fn zero_return_address_stops() {
        let read = chain(&[(0x1000, 0)]);
        assert_eq!(walk(0x1000, read, |_| true, |_, _, _| {}), 0);
    }

    #[test]
    fn cycle_stops_at_max_depth() {
        let read = |address: u64| match address {
            0x1000 | 0x1008 => Some(0x2000),
            0x2000 | 0x2008 => Some(0x1000),
            _ => None,
        };
        let mut frames = 0;
        let count = walk(0x1000, read, |_| true, |_, _, _| frames += 1);
        assert_eq!(count, super::MAX_FRAMES);
        assert_eq!(frames, super::MAX_FRAMES);
    }

    #[test]
    fn self_reference_stops() {
        let read = |address: u64| {
            if address == 0x1000 || address == 0x1008 {
                Some(0x1000)
            } else {
                None
            }
        };
        let mut frames = 0;
        let count = walk(0x1000, read, |_| true, |_, _, _| frames += 1);
        assert_eq!(count, 1);
        assert_eq!(frames, 1);
    }
}
