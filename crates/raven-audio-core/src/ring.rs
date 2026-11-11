// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ring-buffer arithmetic for PCM frames.
//!
//! The helpers here are pure integer functions so `audiod` and the drivers compute the same
//! offsets from the same numbers. A ring is described by its capacity in bytes and two
//! monotonically increasing positions; `position % capacity` is the byte offset. All functions
//! validate their inputs and return `None` or zero instead of panicking.

use crate::format::MAX_CHANNELS;

/// Bytes occupied by `frames` frames of `frame_bytes` bytes, or `None` on overflow.
pub fn frames_to_bytes(frames: usize, frame_bytes: usize) -> Option<usize> {
    frames.checked_mul(frame_bytes)
}

/// Whole frames contained in `bytes`, or `None` when `frame_bytes` is zero.
///
/// A partial trailing frame is dropped; callers that care can compare the remainder with
/// [`bytes_remainder`].
pub fn bytes_to_frames(bytes: usize, frame_bytes: usize) -> Option<usize> {
    if frame_bytes == 0 {
        return None;
    }
    Some(bytes / frame_bytes)
}

/// Bytes left over after the whole frames in `bytes`, or `None` when `frame_bytes` is zero.
pub fn bytes_remainder(bytes: usize, frame_bytes: usize) -> Option<usize> {
    if frame_bytes == 0 {
        return None;
    }
    Some(bytes % frame_bytes)
}

/// Bytes in one interleaved frame of `channels` PCM16 channels, or `None` for an invalid count.
pub fn frame_bytes(channels: u8) -> Option<usize> {
    if channels == 0 || channels > MAX_CHANNELS {
        return None;
    }
    Some(usize::from(channels) * 2)
}

/// The longest contiguous run at `position` that does not cross the end of the ring.
///
/// Returns zero for an out-of-range position or a zero capacity.
pub fn contiguous(position: usize, wanted: usize, capacity: usize) -> usize {
    if capacity == 0 || position >= capacity {
        return 0;
    }
    wanted.min(capacity - position)
}

/// Move `position` forward by `step` bytes, wrapping inside `capacity`.
///
/// Returns `None` when the capacity is zero.
pub fn advance(position: usize, step: usize, capacity: usize) -> Option<usize> {
    if capacity == 0 {
        return None;
    }
    let offset = position % capacity;
    let delta = step % capacity;
    let room = capacity - offset;
    Some(if delta >= room {
        delta - room
    } else {
        offset + delta
    })
}

/// Free bytes in a single-producer/single-consumer ring.
///
/// `write` and `read` are monotonically increasing counters. Returns `None` when either index
/// is further ahead of the other than `capacity`, which means the indices are inconsistent.
pub fn free_bytes(write: usize, read: usize, capacity: usize) -> Option<usize> {
    let used = used_bytes(write, read, capacity)?;
    Some(capacity - used)
}

/// Occupied bytes in a single-producer/single-consumer ring.
///
/// Returns `None` for inconsistent indices or a zero capacity.
pub fn used_bytes(write: usize, read: usize, capacity: usize) -> Option<usize> {
    if capacity == 0 || write < read {
        return None;
    }
    let used = write - read;
    if used > capacity {
        return None;
    }
    Some(used)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rng::Rng;

    #[test]
    fn frame_conversions_round_trip() {
        assert_eq!(frames_to_bytes(480, 4), Some(1920));
        assert_eq!(bytes_to_frames(1920, 4), Some(480));
        assert_eq!(bytes_remainder(1922, 4), Some(2));
        assert_eq!(frames_to_bytes(0, 4), Some(0));
    }

    #[test]
    fn zero_frame_size_is_rejected() {
        assert_eq!(frames_to_bytes(4, 0), Some(0));
        assert_eq!(bytes_to_frames(4, 0), None);
        assert_eq!(bytes_remainder(4, 0), None);
    }

    #[test]
    fn frame_overflow_is_reported() {
        assert_eq!(frames_to_bytes(usize::MAX, 2), None);
        assert_eq!(frames_to_bytes(usize::MAX / 2, 3), None);
        assert_eq!(frame_bytes(0), None);
        assert_eq!(frame_bytes(9), None);
        assert_eq!(frame_bytes(2), Some(4));
    }

    #[test]
    fn contiguous_runs_stop_at_the_end() {
        assert_eq!(contiguous(0, 10, 16), 10);
        assert_eq!(contiguous(12, 10, 16), 4);
        assert_eq!(contiguous(16, 10, 16), 0);
        assert_eq!(contiguous(0, 10, 0), 0);
        assert_eq!(contiguous(3, 0, 16), 0);
    }

    #[test]
    fn advance_wraps_at_the_capacity() {
        assert_eq!(advance(0, 4, 16), Some(4));
        assert_eq!(advance(12, 4, 16), Some(0));
        assert_eq!(advance(12, 10, 16), Some(6));
        assert_eq!(advance(5, 16, 16), Some(5));
        assert_eq!(advance(5, 3, 0), None);
    }

    #[test]
    fn free_and_used_are_complementary() {
        assert_eq!(used_bytes(10, 4, 16), Some(6));
        assert_eq!(free_bytes(10, 4, 16), Some(10));
        assert_eq!(used_bytes(4, 4, 16), Some(0));
        assert_eq!(free_bytes(4, 4, 16), Some(16));
        assert_eq!(used_bytes(20, 4, 16), Some(16));
        assert_eq!(free_bytes(20, 4, 16), Some(0));
    }

    #[test]
    fn inconsistent_indices_are_rejected() {
        assert_eq!(used_bytes(4, 10, 16), None);
        assert_eq!(used_bytes(21, 4, 16), None);
        assert_eq!(used_bytes(4, 4, 0), None);
        assert_eq!(free_bytes(4, 10, 16), None);
    }

    #[test]
    fn random_positions_never_leave_the_ring() {
        let mut rng = Rng::new(0x51A3);
        for _ in 0..10_000 {
            let capacity = 1 + rng.below(4096) as usize;
            let position = rng.next_u64() as usize;
            let step = rng.next_u64() as usize;
            let moved = advance(position, step, capacity).expect("capacity is non-zero");
            assert!(moved < capacity);
            let run = contiguous(moved, rng.below(4096) as usize, capacity);
            assert!(run <= capacity - moved);
        }
    }
}
