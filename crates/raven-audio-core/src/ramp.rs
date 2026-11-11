// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Click-free linear volume ramps.
//!
//! A ramp changes the gain once per frame and keeps every channel of that frame at the same
//! gain, so a stereo image cannot shift while the volume changes. The first frame carries
//! `start`, the last carries `end`, and the step between frames is at most
//! `ceil(|end - start| / (frames - 1))`, which is the smallest possible change for a linear
//! ramp.

use crate::fixed::clamp_i16;
use crate::format::MAX_CHANNELS;
use crate::mix::GAIN_UNITY;

/// Apply a linear Q15 gain ramp over the frames of an interleaved buffer.
///
/// Returns the number of frames processed. A buffer shorter than one frame, or an invalid
/// channel count, is left untouched and returns zero. A single frame receives `end`. Gains
/// above [`GAIN_UNITY`] are clamped.
pub fn ramp(buffer: &mut [i16], channels: u8, start: u16, end: u16) -> usize {
    if channels == 0 || channels > MAX_CHANNELS {
        return 0;
    }
    let channels = usize::from(channels);
    let frames = buffer.len() / channels;
    if frames == 0 {
        return 0;
    }
    let start = i64::from(start.min(GAIN_UNITY));
    let end = i64::from(end.min(GAIN_UNITY));
    for frame in 0..frames {
        let gain = if frames == 1 {
            end
        } else {
            start + (end - start) * frame as i64 / (frames as i64 - 1)
        };
        let base = frame * channels;
        for sample in &mut buffer[base..base + channels] {
            *sample = clamp_i16((i64::from(*sample) * gain) >> 15);
        }
    }
    frames
}

/// The gain applied to each frame of a ramp, for callers that need the envelope separately.
///
/// Writes at most `out.len()` values and returns the number of frames an equal-length [`ramp`]
/// would process.
pub fn envelope(frames: usize, start: u16, end: u16, out: &mut [u16]) -> usize {
    let count = frames.min(out.len());
    let start_i = i64::from(start.min(GAIN_UNITY));
    let end_i = i64::from(end.min(GAIN_UNITY));
    for (frame, slot) in out.iter_mut().take(count).enumerate() {
        let gain = if frames <= 1 {
            end_i
        } else {
            start_i + (end_i - start_i) * frame as i64 / (frames as i64 - 1)
        };
        *slot = gain as u16;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn ramp_endpoints_are_exact() {
        let mut buffer = vec![i16::MAX; 6];
        assert_eq!(ramp(&mut buffer, 2, 0, GAIN_UNITY), 3);
        assert_eq!(buffer[0], 0);
        assert_eq!(buffer[1], 0);
        assert_ne!(buffer[2], buffer[0]);
        assert_eq!(buffer[4], i16::MAX);
        assert_eq!(buffer[5], i16::MAX);
    }

    #[test]
    fn envelope_is_linear_and_monotone() {
        let mut gains = [0u16; 9];
        assert_eq!(envelope(9, 0, GAIN_UNITY, &mut gains), 9);
        assert_eq!(gains[0], 0);
        assert_eq!(gains[8], GAIN_UNITY);
        for pair in gains.windows(2) {
            assert!(pair[0] <= pair[1], "{gains:?}");
        }
        let step = gains[1] - gains[0];
        for pair in gains.windows(2) {
            assert!(pair[1] - pair[0] <= step + 1, "{gains:?}");
        }
    }

    #[test]
    fn descending_ramp_is_monotone() {
        let mut buffer = vec![10_000i16; 40];
        assert_eq!(ramp(&mut buffer, 1, GAIN_UNITY, 0), 40);
        for pair in buffer.windows(2) {
            assert!(pair[0] >= pair[1], "{buffer:?}");
        }
        assert_eq!(buffer[0], 10_000);
        assert_eq!(buffer[39], 0);
    }

    #[test]
    fn every_channel_of_a_frame_shares_the_gain() {
        let original = [1000i16, -1000, 2000, -2000, 3000, -3000];
        let mut buffer = original;
        assert_eq!(ramp(&mut buffer, 2, GAIN_UNITY / 2, GAIN_UNITY / 2), 3);
        assert_eq!(buffer, [500, -500, 1000, -1000, 1500, -1500]);
    }

    #[test]
    fn clamp_limits_the_gain_to_unity() {
        let original = [i16::MIN, 1234, i16::MAX];
        let mut buffer = original;
        assert_eq!(ramp(&mut buffer, 1, u16::MAX, u16::MAX), 3);
        assert_eq!(buffer, original);
    }

    #[test]
    fn single_frame_uses_the_end_gain() {
        let mut buffer = [1000i16];
        assert_eq!(ramp(&mut buffer, 1, 0, GAIN_UNITY / 2), 1);
        assert_eq!(buffer, [500]);
    }

    #[test]
    fn invalid_inputs_are_ignored() {
        let mut buffer = [7i16; 4];
        assert_eq!(ramp(&mut buffer, 0, 0, GAIN_UNITY), 0);
        assert_eq!(ramp(&mut buffer, 9, 0, GAIN_UNITY), 0);
        assert_eq!(ramp(&mut [], 2, 0, GAIN_UNITY), 0);
        let mut short = [7i16; 3];
        assert_eq!(ramp(&mut short, 2, 0, GAIN_UNITY), 1);
        assert_eq!(short, [7, 7, 7]);
    }
}
