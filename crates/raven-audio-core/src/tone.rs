// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic tone and silence generator.
//!
//! The sine comes from a 1024-entry Q15 table built at compile time from [`sin_q32`]. The phase
//! is a `u32` accumulator advanced by `(hz << 32) / rate` per sample, so a frequency whose
//! period is a power of two in samples repeats bit for bit, while other frequencies advance
//! with the same integer step on every run. There is no floating point and no hidden state.

use crate::fixed::{TAU_Q32, sin_q32};
use crate::format::{MAX_CHANNELS, MAX_RATE, MIN_RATE};

/// Entries in the Q15 sine table (one full period).
pub const SINE_LEN: usize = 1024;

const fn build_sine_table() -> [i16; SINE_LEN] {
    let mut table = [0i16; SINE_LEN];
    let mut index = 0usize;
    while index < SINE_LEN {
        let radians = (index as i64) * TAU_Q32 / SINE_LEN as i64;
        let value = sin_q32(radians) >> 17;
        table[index] = if value > i16::MAX as i64 {
            i16::MAX
        } else if value < i16::MIN as i64 {
            i16::MIN
        } else {
            value as i16
        };
        index += 1;
    }
    table
}

/// One period of `sin` in Q15, with index 0 at phase zero.
pub static SINE_Q15: [i16; SINE_LEN] = build_sine_table();

/// Waveform shapes the generator can produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    /// Table-based sine.
    Sine,
    /// Square wave: `+amplitude` for the first half of the period, `-amplitude` for the rest.
    Square,
    /// Constant zero, regardless of frequency.
    Silence,
}

/// An integer tone generator producing one interleaved PCM16 sample at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tone {
    waveform: Waveform,
    phase: u32,
    step: u32,
    amplitude: i16,
}

impl Tone {
    /// Build a generator.
    ///
    /// Returns `None` when the rate is outside `8000..=192000`, when `hz` is zero or above the
    /// Nyquist frequency, or when `amplitude` is negative.
    pub fn new(waveform: Waveform, rate: u32, hz: u32, amplitude: i16) -> Option<Self> {
        if !(MIN_RATE..=MAX_RATE).contains(&rate) {
            return None;
        }
        if hz == 0 || hz > rate / 2 {
            return None;
        }
        if amplitude < 0 {
            return None;
        }
        let step = ((u64::from(hz) << 32) / u64::from(rate)) as u32;
        if step == 0 {
            return None;
        }
        Some(Self {
            waveform,
            phase: 0,
            step,
            amplitude,
        })
    }

    /// The waveform this generator produces.
    pub fn waveform(&self) -> Waveform {
        self.waveform
    }

    /// The phase step per sample, in `u32` phase units.
    pub fn step(&self) -> u32 {
        self.step
    }

    /// Restart at phase zero.
    pub fn reset(&mut self) {
        self.phase = 0;
    }

    /// Produce the next sample.
    pub fn next_sample(&mut self) -> i16 {
        let sample = match self.waveform {
            Waveform::Sine => {
                let index = (self.phase >> 22) as usize;
                let entry = SINE_Q15[index];
                ((i32::from(entry) * i32::from(self.amplitude)) >> 15) as i16
            }
            Waveform::Square => {
                if self.phase < 0x8000_0000 {
                    self.amplitude
                } else {
                    -self.amplitude
                }
            }
            Waveform::Silence => 0,
        };
        self.phase = self.phase.wrapping_add(self.step);
        sample
    }

    /// Fill a buffer; one sample per element, for any channel count.
    pub fn fill(&mut self, out: &mut [i16]) {
        for slot in out {
            *slot = self.next_sample();
        }
    }

    /// Render interleaved frames: the same sample is copied to every channel of a frame.
    ///
    /// Returns the number of frames written. An invalid channel count writes nothing.
    pub fn fill_frames(&mut self, out: &mut [i16], channels: u8) -> usize {
        if channels == 0 || channels > MAX_CHANNELS {
            return 0;
        }
        let channels = usize::from(channels);
        let frames = out.len() / channels;
        for frame in 0..frames {
            let value = self.next_sample();
            let base = frame * channels;
            for slot in &mut out[base..base + channels] {
                *slot = value;
            }
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn the_sine_table_hits_the_cardinal_points() {
        assert_eq!(SINE_Q15[0], 0);
        assert!(
            SINE_Q15[SINE_LEN / 4] >= 32_766,
            "{}",
            SINE_Q15[SINE_LEN / 4]
        );
        assert!(SINE_Q15[SINE_LEN / 2].abs() <= 1);
        assert!(SINE_Q15[3 * SINE_LEN / 4] <= -32_766);
    }

    #[test]
    fn the_sine_table_is_odd_and_smooth() {
        for index in 1..SINE_LEN / 2 {
            let mirrored = SINE_Q15[SINE_LEN - index];
            assert!(
                (i32::from(SINE_Q15[index]) + i32::from(mirrored)).abs() <= 1,
                "index {index}"
            );
        }
        for pair in SINE_Q15.windows(2) {
            assert!(
                (i32::from(pair[1]) - i32::from(pair[0])).abs() < 250,
                "table has a jump: {pair:?}"
            );
        }
    }

    #[test]
    fn a_sine_with_a_power_of_two_period_is_exact() {
        let mut tone = Tone::new(Waveform::Sine, 48_000, 6_000, 16_384).expect("valid tone");
        let mut samples = vec![0i16; 8 * 64];
        tone.fill(&mut samples);
        for index in 8..samples.len() {
            assert_eq!(samples[index], samples[index - 8], "sample {index}");
        }
    }

    #[test]
    fn a_square_wave_with_a_power_of_two_period_is_exact() {
        let mut tone = Tone::new(Waveform::Square, 8_000, 1_000, 10_000).expect("valid tone");
        let mut samples = vec![0i16; 8 * 16];
        tone.fill(&mut samples);
        assert_eq!(&samples[..4], &[10_000; 4]);
        assert_eq!(&samples[4..8], &[-10_000; 4]);
        for index in 8..samples.len() {
            assert_eq!(samples[index], samples[index - 8], "sample {index}");
        }
    }

    #[test]
    fn silence_is_always_zero() {
        let mut tone = Tone::new(Waveform::Silence, 44_100, 440, 32_767).expect("valid tone");
        let mut samples = [-1i16; 100];
        tone.fill(&mut samples);
        assert_eq!(samples, [0; 100]);
    }

    #[test]
    fn sine_starts_at_zero_and_stays_in_range() {
        let mut tone = Tone::new(Waveform::Sine, 44_100, 440, 32_767).expect("valid tone");
        let mut samples = vec![0i16; 44_100];
        tone.fill(&mut samples);
        assert!(samples[0].abs() <= 16);
        for sample in &samples {
            assert!((-32_767..=32_767).contains(sample));
        }
        let peak = samples.iter().map(|s| i32::from(*s).abs()).max().unwrap();
        assert!(peak >= 32_000, "peak {peak}");
    }

    #[test]
    fn zero_amplitude_is_silence_for_every_waveform() {
        for waveform in [Waveform::Sine, Waveform::Square] {
            let mut tone = Tone::new(waveform, 48_000, 440, 0).expect("valid tone");
            let mut samples = [9i16; 32];
            tone.fill(&mut samples);
            assert_eq!(samples, [0; 32], "{waveform:?}");
        }
    }

    #[test]
    fn frame_fill_repeats_one_sample_per_frame() {
        let mut tone = Tone::new(Waveform::Sine, 48_000, 6_000, 16_384).expect("valid tone");
        let mut frames = [0i16; 8];
        assert_eq!(tone.fill_frames(&mut frames, 2), 4);
        assert_eq!(frames[0], frames[1]);
        assert_eq!(frames[2], frames[3]);
        assert_ne!(frames[0], frames[2]);
        let mut invalid = [5i16; 4];
        assert_eq!(tone.fill_frames(&mut invalid, 0), 0);
        assert_eq!(tone.fill_frames(&mut invalid, 9), 0);
    }

    #[test]
    fn reset_restarts_the_phase() {
        let mut tone = Tone::new(Waveform::Square, 8_000, 1_000, 12_345).expect("valid tone");
        let mut first = [0i16; 20];
        let mut second = [0i16; 20];
        tone.fill(&mut first);
        tone.reset();
        tone.fill(&mut second);
        assert_eq!(first, second);
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(Tone::new(Waveform::Sine, 7_999, 440, 1000).is_none());
        assert!(Tone::new(Waveform::Sine, 192_001, 440, 1000).is_none());
        assert!(Tone::new(Waveform::Sine, 48_000, 0, 1000).is_none());
        assert!(Tone::new(Waveform::Sine, 48_000, 24_001, 1000).is_none());
        assert!(Tone::new(Waveform::Sine, 48_000, 440, -1).is_none());
        assert!(Tone::new(Waveform::Square, 48_000, 440, i16::MIN).is_none());
        assert!(Tone::new(Waveform::Sine, 8_000, 4_000, 1000).is_some());
    }

    #[test]
    fn step_matches_the_requested_frequency() {
        let tone = Tone::new(Waveform::Sine, 48_000, 6_000, 1000).expect("valid tone");
        assert_eq!(tone.step(), u32::MAX / 8 + 1);
        assert_eq!(tone.waveform(), Waveform::Sine);
        let other = Tone::new(Waveform::Sine, 44_100, 440, 1000).expect("valid tone");
        assert_eq!(other.step(), ((440u64 << 32) / 44_100) as u32);
    }
}
