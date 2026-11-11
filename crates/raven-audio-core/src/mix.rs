// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Saturating N-way mixer with Q15 per-stream gain.
//!
//! The accumulator is `i64`, so eight full-scale streams at unity gain cannot overflow the
//! intermediate sum. The final clamp makes the mix saturate instead of wrapping, and because
//! every term is an integer sum the result does not depend on the order of `streams`.

use crate::fixed::clamp_i16;

/// Unity gain in Q15. Values above it are clamped by [`mix`].
pub const GAIN_UNITY: u16 = 32_768;

/// One input stream for [`mix`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stream<'a> {
    /// Interleaved PCM16 samples. Samples past the end of a shorter stream mix as silence.
    pub samples: &'a [i16],
    /// Gain in Q15; [`GAIN_UNITY`] is 1.0 and larger values are clamped to it.
    pub gain: u16,
}

impl<'a> Stream<'a> {
    /// Build a stream record.
    pub const fn new(samples: &'a [i16], gain: u16) -> Self {
        Self { samples, gain }
    }

    /// The gain actually applied, with values above unity clamped.
    pub const fn effective_gain(&self) -> u16 {
        if self.gain > GAIN_UNITY {
            GAIN_UNITY
        } else {
            self.gain
        }
    }
}

/// Mix `streams` into `out`, saturating on overflow.
///
/// Streams shorter than `out` contribute silence after their end; `out` is fully written.
/// An empty `streams` slice clears `out`. The result is independent of the stream order.
pub fn mix(streams: &[Stream<'_>], out: &mut [i16]) {
    for (index, slot) in out.iter_mut().enumerate() {
        let mut sum = 0i64;
        for stream in streams {
            if let Some(sample) = stream.samples.get(index) {
                sum += (i64::from(*sample) * i64::from(stream.effective_gain())) >> 15;
            }
        }
        *slot = clamp_i16(sum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rng::Rng;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn unity_gain_copies_one_stream() {
        let input = [i16::MIN, -1, 0, 1, i16::MAX];
        let mut out = [0i16; 5];
        mix(&[Stream::new(&input, GAIN_UNITY)], &mut out);
        assert_eq!(out, input);
    }

    #[test]
    fn half_gain_halves_every_sample() {
        let input = [1000i16, -1000, 32767];
        let mut out = [0i16; 3];
        mix(&[Stream::new(&input, GAIN_UNITY / 2)], &mut out);
        assert_eq!(out, [500, -500, 16383]);
    }

    #[test]
    fn silence_gain_writes_silence() {
        let input = [i16::MAX; 8];
        let mut out = [-1i16; 8];
        mix(&[Stream::new(&input, 0)], &mut out);
        assert_eq!(out, [0; 8]);
    }

    #[test]
    fn two_full_scale_streams_saturate_instead_of_wrapping() {
        let input = [i16::MAX; 4];
        let mut out = [0i16; 4];
        mix(
            &[
                Stream::new(&input, GAIN_UNITY),
                Stream::new(&input, GAIN_UNITY),
            ],
            &mut out,
        );
        assert_eq!(out, [i16::MAX; 4]);

        let input = [i16::MIN; 4];
        mix(
            &[
                Stream::new(&input, GAIN_UNITY),
                Stream::new(&input, GAIN_UNITY),
            ],
            &mut out,
        );
        assert_eq!(out, [i16::MIN; 4]);
    }

    #[test]
    fn gains_above_unity_are_clamped() {
        let input = [1000i16, -1000];
        let mut out = [0i16; 2];
        mix(&[Stream::new(&input, u16::MAX)], &mut out);
        assert_eq!(out, input);
        assert_eq!(Stream::new(&input, u16::MAX).effective_gain(), GAIN_UNITY);
    }

    #[test]
    fn shorter_streams_are_zero_padded() {
        let short = [100i16];
        let long = [10i16, 20, 30];
        let mut out = [0i16; 3];
        mix(
            &[
                Stream::new(&short, GAIN_UNITY),
                Stream::new(&long, GAIN_UNITY),
            ],
            &mut out,
        );
        assert_eq!(out, [110, 20, 30]);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn no_streams_clears_the_output() {
        let mut out = [7i16; 4];
        mix(&[], &mut out);
        assert_eq!(out, [0; 4]);
    }

    #[test]
    fn ten_thousand_random_mixes_do_not_overflow_and_are_commutative() {
        let mut rng = Rng::new(0xB53);
        for round in 0..10_000 {
            let count = 1 + rng.below(5) as usize;
            let length = rng.below(24) as usize;
            let streams: Vec<(Vec<i16>, u16)> = (0..count)
                .map(|_| {
                    let mut samples = Vec::with_capacity(length);
                    for _ in 0..length {
                        samples.push(rng.extreme_sample());
                    }
                    (samples, rng.below(u32::from(u16::MAX) + 1) as u16)
                })
                .collect();
            let records: Vec<Stream<'_>> = streams
                .iter()
                .map(|(samples, gain)| Stream::new(samples, *gain))
                .collect();
            let mut forward = vec![0i16; length];
            mix(&records, &mut forward);

            let mut reversed = records.clone();
            reversed.reverse();
            let mut backward = vec![0i16; length];
            mix(&reversed, &mut backward);
            assert_eq!(forward, backward, "round {round} is order dependent");

            for sample in &forward {
                assert!((-32_768..=32_767).contains(&i32::from(*sample)));
            }
        }
    }
}
