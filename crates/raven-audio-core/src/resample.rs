// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integer resampling between sample rates.
//!
//! [`resample_linear`] is the cheap path: one multiply and one shift per sample, a rational
//! position `n * from / to`, linear interpolation between the two neighbouring input samples.
//! [`SincResampler`] is the quality path: a windowed-sinc low-pass, evaluated at
//! [`SINC_PHASES`] fractional offsets and [`SINC_TAPS`] taps, all in Q15. When the output rate
//! is lower than the input rate the kernel is scaled to the output Nyquist
//! (`cutoff = to / from`), so the resampler band-limits instead of aliasing.
//!
//! Both paths obey the same length rule: `out = round(in * to / from)`, with halves rounded
//! up. Output frames whose filter window reaches past either end of the input see zero-padded
//! samples, which is a transient confined to `SINC_TAPS / 2` frames on each side.
//!
//! A polyphase row is normalised to a DC gain of exactly 32768/32768. At equal rates every row
//! is a unit impulse, but Q15 has no 32768 tap, so the single residual unit is spread over the
//! nearest usable taps and a same-rate pass may differ from the input by at most one LSB.

use alloc::vec::Vec;

use crate::fixed::{PI_Q32, Q30_ONE, TAU_Q32, clamp_i16, cos_q32, sin_q32};
use crate::format::MAX_CHANNELS;

/// Phase steps in the polyphase coefficient table.
pub const SINC_PHASES: usize = 256;
/// Taps in the polyphase filter; an odd count keeps the kernel centred on the output position.
pub const SINC_TAPS: usize = 65;
/// Taps on each side of the centre.
const SINC_HALF: usize = SINC_TAPS / 2;
/// Q16 representation of 1.0.
const Q16_ONE: i64 = 1 << 16;

/// Measured worst-case interpolation error of [`SincResampler`], in LSB of a PCM16 sample.
///
/// Measured with a 0.5 full-scale (16 384 LSB) sine at 100 Hz, 440 Hz, 1 kHz, 2 kHz, 3 kHz,
/// 5 kHz, 8 kHz, 10 kHz, 12 kHz and 15 kHz, converted in both directions between 44 100 Hz and
/// 48 000 Hz, ignoring the first and last 64 output frames where the zero-padded filter window
/// is still settling. The worst observed value is 173 LSB (15 kHz, 48 kHz to 44.1 kHz); above
/// 15 kHz the error grows towards the transition band. `tests` assert the bound. For comparison,
/// linear interpolation of the same 8 kHz tone errs by about 2 600 LSB.
pub const SINC_MAX_ERROR_LSB: i32 = 200;

/// Number of interleaved frames produced for `input_frames` frames.
///
/// `out = round(input * to / from)` with halves rounded up. Returns `None` when a rate is zero
/// or the result does not fit in `usize`.
pub fn output_frames(input_frames: usize, from: u32, to: u32) -> Option<usize> {
    if from == 0 || to == 0 {
        return None;
    }
    let scaled = (input_frames as u128) * u128::from(to) + u128::from(from) / 2;
    usize::try_from(scaled / u128::from(from)).ok()
}

fn valid_channels(channels: u8) -> Option<usize> {
    if channels == 0 || channels > MAX_CHANNELS {
        return None;
    }
    Some(usize::from(channels))
}

fn allocate(length: usize) -> Option<Vec<i16>> {
    let mut out = Vec::new();
    out.try_reserve_exact(length).ok()?;
    out.resize(length, 0);
    Some(out)
}

/// Resample with rational linear interpolation.
///
/// Returns `None` for a zero rate, an invalid channel count or an allocation failure. The
/// output length follows [`output_frames`]. A trailing incomplete frame in `input` is ignored.
pub fn resample_linear(input: &[i16], channels: u8, from: u32, to: u32) -> Option<Vec<i16>> {
    let channels = valid_channels(channels)?;
    let input_frames = input.len() / channels;
    let output_frames = output_frames(input_frames, from, to)?;
    let mut out = allocate(output_frames.checked_mul(channels)?)?;
    if input_frames == 0 {
        return Some(out);
    }
    let denominator = u128::from(to);
    for frame in 0..output_frames {
        let position = (frame as u128) * u128::from(from);
        let mut index = usize::try_from(position / denominator).ok()?;
        let mut fraction = ((position % denominator) << 16) / denominator;
        if index >= input_frames {
            index = input_frames - 1;
            fraction = 0;
        }
        let next = (index + 1).min(input_frames - 1);
        let base = index * channels;
        let next_base = next * channels;
        for channel in 0..channels {
            let first = i64::from(input[base + channel]);
            let second = i64::from(input[next_base + channel]);
            let value = first + (((second - first) * fraction as i64) >> 16);
            out[frame * channels + channel] = clamp_i16(value);
        }
    }
    Some(out)
}

/// Cutoff of the anti-imaging/anti-aliasing kernel in Q16 of the input Nyquist.
fn cutoff_q16(from: u32, to: u32) -> i64 {
    if to >= from {
        Q16_ONE
    } else {
        (i64::from(to) << 16) / i64::from(from)
    }
}

/// Blackman window at `tap`, Q30. The first and last taps are exactly zero.
fn window_q30(tap: usize) -> i64 {
    let span = (SINC_TAPS - 1) as i64;
    let position = (tap as i64 * Q16_ONE) / span;
    let angle = ((position as i128 * i128::from(TAU_Q32)) >> 16) as i64;
    let cosine = cos_q32(angle);
    let cosine_double = cos_q32(angle * 2);
    let value = 450_971_566i128 - i128::from(cosine) / 2
        + ((i128::from(cosine_double) * 85_899_346i128) >> 30);
    value.clamp(0, i128::from(Q30_ONE)) as i64
}

/// `sinc(pi * x)` in Q30, where `x` is `offset * cutoff` in Q16 input samples.
fn sinc_q30(offset_q16: i64, cutoff: i64) -> i64 {
    let x_q16 = (i128::from(offset_q16) * i128::from(cutoff)) >> 16;
    if x_q16 == 0 {
        return Q30_ONE;
    }
    let radians = (x_q16 * i128::from(PI_Q32)) >> 16;
    if radians == 0 {
        return Q30_ONE;
    }
    let sine = i128::from(sin_q32(radians as i64));
    ((sine << 30) / radians) as i64
}

/// Build the `SINC_PHASES` normalised Q15 rows of the polyphase kernel.
fn build_coefficients(from: u32, to: u32) -> Option<Vec<i16>> {
    let cutoff = cutoff_q16(from, to);
    let mut table = Vec::new();
    table.try_reserve_exact(SINC_PHASES * SINC_TAPS).ok()?;
    let mut phase = 0usize;
    while phase < SINC_PHASES {
        let fraction = (phase as i64 * Q16_ONE) / SINC_PHASES as i64;
        let mut taps = [0i64; SINC_TAPS];
        let mut sum = 0i64;
        let mut tap = 0usize;
        while tap < SINC_TAPS {
            let offset = (tap as i64 - SINC_HALF as i64) * Q16_ONE - fraction;
            let coefficient = (((sinc_q30(offset, cutoff) * window_q30(tap)) >> 30) * cutoff) >> 16;
            taps[tap] = coefficient;
            sum += coefficient;
            tap += 1;
        }
        if sum == 0 {
            taps = [0i64; SINC_TAPS];
            taps[SINC_HALF] = Q30_ONE;
            sum = Q30_ONE;
        }
        let mut row = [0i16; SINC_TAPS];
        let mut sum_q15 = 0i32;
        let mut tap = 0usize;
        while tap < SINC_TAPS {
            let bias = if taps[tap] >= 0 { sum / 2 } else { -sum / 2 };
            let scaled = (i128::from(taps[tap]) * 32_768 + i128::from(bias)) / i128::from(sum);
            let value = scaled.clamp(i128::from(i16::MIN), i128::from(i16::MAX)) as i16;
            row[tap] = value;
            sum_q15 += i32::from(value);
            tap += 1;
        }
        // Q15 cannot express a row sum of 32768 with a single 32768 tap. The rounding residual
        // is spread over the taps that can absorb it (largest magnitude first), which pins the
        // DC gain to exactly 1.0 instead of letting the row drift by a few LSB.
        let mut remaining = 32_768 - sum_q15;
        while remaining != 0 {
            let step: i16 = if remaining > 0 { 1 } else { -1 };
            let mut chosen = usize::MAX;
            let mut chosen_abs = -1i32;
            let mut chosen_distance = i32::MAX;
            for (index, value) in row.iter().enumerate() {
                let candidate = i32::from(*value) + i32::from(step);
                if candidate > i32::from(i16::MAX) || candidate < i32::from(i16::MIN) {
                    continue;
                }
                let magnitude = i32::from(*value).abs();
                let distance = (index as i32 - SINC_HALF as i32).abs();
                if magnitude > chosen_abs || (magnitude == chosen_abs && distance < chosen_distance)
                {
                    chosen_abs = magnitude;
                    chosen_distance = distance;
                    chosen = index;
                }
            }
            if chosen == usize::MAX {
                break;
            }
            row[chosen] = (i32::from(row[chosen]) + i32::from(step)) as i16;
            remaining -= i32::from(step);
        }
        table.extend_from_slice(&row);
        phase += 1;
    }
    Some(table)
}

/// A polyphase windowed-sinc resampler for one rate pair and channel count.
///
/// The coefficient table is built once in [`SincResampler::new`] and reused by every
/// [`SincResampler::process`] call, so `audiod` can keep one instance per stream. All arithmetic
/// is integer; the cutoff accounts for downsampling so high frequencies do not fold back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SincResampler {
    from: u32,
    to: u32,
    channels: u8,
    coefficients: Vec<i16>,
}

impl SincResampler {
    /// Build the coefficient table for `from` Hz to `to` Hz.
    ///
    /// Returns `None` for a zero rate, an invalid channel count or an allocation failure.
    pub fn new(from: u32, to: u32, channels: u8) -> Option<Self> {
        let channels = valid_channels(channels)?;
        if from == 0 || to == 0 {
            return None;
        }
        Some(Self {
            from,
            to,
            channels: channels as u8,
            coefficients: build_coefficients(from, to)?,
        })
    }

    /// Input rate in Hz.
    pub fn from_rate(&self) -> u32 {
        self.from
    }

    /// Output rate in Hz.
    pub fn to_rate(&self) -> u32 {
        self.to
    }

    /// Interleaved channel count.
    pub fn channels(&self) -> u8 {
        self.channels
    }

    /// Number of output frames for `input_frames` input frames.
    pub fn output_frames(&self, input_frames: usize) -> Option<usize> {
        output_frames(input_frames, self.from, self.to)
    }

    /// Resample an interleaved buffer.
    ///
    /// Returns `None` when the buffer length overflows or on an allocation failure. A trailing
    /// incomplete frame is ignored.
    pub fn process(&self, input: &[i16]) -> Option<Vec<i16>> {
        let channels = usize::from(self.channels);
        let input_frames = input.len() / channels;
        let output_frames = self.output_frames(input_frames)?;
        let mut out = allocate(output_frames.checked_mul(channels)?)?;
        if input_frames == 0 {
            return Some(out);
        }
        let denominator = u128::from(self.to);
        let divisor = i64::try_from(denominator).ok()?;
        for frame in 0..output_frames {
            let position = (frame as u128) * u128::from(self.from);
            let mut index = usize::try_from(position / denominator).ok()?;
            let remainder = position % denominator;
            let scaled = remainder * SINC_PHASES as u128;
            let mut phase = usize::try_from(scaled / denominator).ok()?;
            let weight = i64::try_from(scaled % denominator).ok()?;
            if index >= input_frames {
                index = input_frames - 1;
                phase = 0;
            }
            if phase >= SINC_PHASES {
                phase = SINC_PHASES - 1;
            }
            // Blend the two neighbouring phase rows; fraction 1.0 of the last row is phase 0
            // of the next input frame.
            let (next_phase, next_index) = if phase + 1 < SINC_PHASES {
                (phase + 1, index)
            } else {
                (0, (index + 1).min(input_frames - 1))
            };
            let first = &self.coefficients[phase * SINC_TAPS..(phase + 1) * SINC_TAPS];
            let second = &self.coefficients[next_phase * SINC_TAPS..(next_phase + 1) * SINC_TAPS];
            let low_weight = divisor - weight;
            for channel in 0..channels {
                let mut low = 0i64;
                let mut high = 0i64;
                for (tap, (first_tap, second_tap)) in first.iter().zip(second.iter()).enumerate() {
                    let offset = tap as isize - SINC_HALF as isize;
                    let first_source = index as isize + offset;
                    if first_source >= 0 && first_source < input_frames as isize {
                        low += i64::from(*first_tap)
                            * i64::from(input[first_source as usize * channels + channel]);
                    }
                    let second_source = next_index as isize + offset;
                    if second_source >= 0 && second_source < input_frames as isize {
                        high += i64::from(*second_tap)
                            * i64::from(input[second_source as usize * channels + channel]);
                    }
                }
                // Linear blending of the two rows removes the 1/256 phase quantisation error
                // at the cost of one extra convolution.
                let blended = (low * low_weight + high * weight) / divisor;
                out[frame * channels + channel] = clamp_i16((blended + (1 << 14)) >> 15);
            }
        }
        Some(out)
    }
}

/// Build a [`SincResampler`] and resample in one call.
pub fn resample_sinc(input: &[i16], channels: u8, from: u32, to: u32) -> Option<Vec<i16>> {
    SincResampler::new(from, to, channels)?.process(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rng::Rng;
    use crate::tone::{Tone, Waveform};
    use alloc::vec;

    const AMPLITUDE: i16 = 16_384;
    const ERROR_FREQUENCIES: [u32; 10] = [
        100, 440, 1_000, 2_000, 3_000, 5_000, 8_000, 10_000, 12_000, 15_000,
    ];

    fn sine_input(rate: u32, hz: u32, frames: usize) -> Vec<i16> {
        let mut samples = vec![0i16; frames];
        let mut tone = Tone::new(Waveform::Sine, rate, hz, AMPLITUDE).expect("valid tone");
        tone.fill(&mut samples);
        samples
    }

    /// A high-precision sine from the Q32 kernel, used as the resampler reference.
    ///
    /// The 1024-entry tone table quantises the phase of the input and would otherwise dominate
    /// the measured interpolation error.
    fn precise_sine(rate: u32, hz: u32, frames: usize) -> Vec<i16> {
        let mut samples = Vec::with_capacity(frames);
        for frame in 0..frames {
            let phase = (frame as i128 * i128::from(hz) % i128::from(rate))
                * i128::from(crate::fixed::TAU_Q32)
                / i128::from(rate);
            let value = (i128::from(sin_q32(phase as i64)) * i128::from(AMPLITUDE)) >> 32;
            samples.push(value as i16);
        }
        samples
    }

    fn worst_sine_error(from: u32, to: u32, hz: u32) -> i32 {
        let input = precise_sine(from, hz, (from / 10) as usize);
        let output = resample_sinc(&input, 1, from, to).expect("resample");
        let expected = precise_sine(to, hz, output.len());
        let edge = 64;
        let mut worst = 0;
        for (actual, reference) in output[edge..output.len() - edge]
            .iter()
            .zip(expected[edge..expected.len() - edge].iter())
        {
            worst = worst.max((i32::from(*actual) - i32::from(*reference)).abs());
        }
        worst
    }

    #[test]
    fn length_rule_rounds_halves_up() {
        assert_eq!(output_frames(441, 44_100, 48_000), Some(480));
        assert_eq!(output_frames(480, 48_000, 44_100), Some(441));
        assert_eq!(output_frames(0, 44_100, 48_000), Some(0));
        assert_eq!(output_frames(1, 48_000, 24_000), Some(1));
        assert_eq!(output_frames(3, 48_000, 32_000), Some(2));
        assert_eq!(output_frames(1, 2_000, 3_000), Some(2));
        assert_eq!(output_frames(10, 0, 48_000), None);
        assert_eq!(output_frames(10, 48_000, 0), None);
    }

    #[test]
    fn length_rule_property_holds_for_random_lengths() {
        let mut rng = Rng::new(0x4E51);
        let rates = [8_000u32, 11_025, 22_050, 44_100, 48_000, 96_000, 192_000];
        for _ in 0..10_000 {
            let input_frames = rng.below(20_000) as usize;
            let from = rates[rng.below(rates.len() as u32) as usize];
            let to = rates[rng.below(rates.len() as u32) as usize];
            let expected = ((input_frames as u128 * u128::from(to) + u128::from(from) / 2)
                / u128::from(from)) as usize;
            assert_eq!(output_frames(input_frames, from, to), Some(expected));
        }
    }

    #[test]
    fn linear_identity_rate_copies_the_buffer() {
        let input = sine_input(48_000, 440, 100);
        let output = resample_linear(&input, 1, 48_000, 48_000).expect("resample");
        assert_eq!(output, input);
    }

    #[test]
    fn linear_interpolates_between_neighbours() {
        let input = [0i16, 1000, 2000, 3000];
        let output = resample_linear(&input, 1, 2, 4).expect("resample");
        assert_eq!(output, [0, 500, 1000, 1500, 2000, 2500, 3000, 3000]);
    }

    #[test]
    fn linear_keeps_dc_constant() {
        let input = [1234i16; 64];
        let output = resample_linear(&input, 1, 44_100, 48_000).expect("resample");
        assert!(output.iter().all(|sample| *sample == 1234));
    }

    #[test]
    fn linear_handles_a_tail_that_rounds_past_the_input() {
        let output = resample_linear(&[1000i16], 1, 2, 3).expect("resample");
        assert_eq!(output, [1000, 1000]);
    }

    #[test]
    fn linear_preserves_stereo_channel_separation() {
        let input = [100i16, -100, 200, -200];
        let output = resample_linear(&input, 2, 2, 4).expect("resample");
        assert_eq!(output.len(), 8);
        for frame in 0..4 {
            assert_eq!(output[frame * 2], -output[frame * 2 + 1]);
        }
    }

    #[test]
    fn sinc_same_rate_stays_within_one_lsb() {
        let input = sine_input(48_000, 440, 200);
        let output = resample_sinc(&input, 1, 48_000, 48_000).expect("resample");
        assert_eq!(output.len(), input.len());
        for (index, (actual, expected)) in output.iter().zip(input.iter()).enumerate() {
            let difference = (i32::from(*actual) - i32::from(*expected)).abs();
            assert!(difference <= 1, "frame {index} differs by {difference}");
        }
    }

    #[test]
    fn sinc_passes_dc_through_the_middle() {
        let input = [2000i16; 1024];
        let output = resample_sinc(&input, 1, 44_100, 48_000).expect("resample");
        let edge = SINC_HALF + 4;
        for (index, sample) in output[edge..output.len() - edge].iter().enumerate() {
            assert!((i32::from(*sample) - 2000).abs() <= 2, "frame {index}");
        }
    }

    #[test]
    fn sinc_keeps_stereo_channels_independent() {
        let mut input = vec![0i16; 400];
        for frame in 0..200 {
            input[frame * 2] = 1000;
            input[frame * 2 + 1] = -1000;
        }
        let output = resample_sinc(&input, 2, 48_000, 44_100).expect("resample");
        let edge = SINC_HALF + 4;
        for frame in edge..output.len() / 2 - edge {
            assert!(
                (i32::from(output[frame * 2]) - 1000).abs() <= 2,
                "left {frame}"
            );
            assert!(
                (i32::from(output[frame * 2 + 1]) + 1000).abs() <= 2,
                "right {frame}"
            );
        }
    }

    #[test]
    fn sinc_output_length_follows_the_rule() {
        let mut rng = Rng::new(0x51AC);
        for (from, to) in [
            (44_100u32, 48_000u32),
            (48_000, 44_100),
            (8_000, 192_000),
            (192_000, 8_000),
            (22_050, 44_100),
        ] {
            let resampler = SincResampler::new(from, to, 2).expect("resampler");
            for _ in 0..40 {
                let frames = rng.below(512) as usize;
                let input = vec![0i16; frames * 2];
                let output = resampler.process(&input).expect("process");
                assert_eq!(output.len(), output_frames(frames, from, to).unwrap() * 2);
            }
        }
    }

    #[test]
    fn sinc_44100_to_48000_sine_error_is_within_the_bound() {
        for hz in ERROR_FREQUENCIES {
            let worst = worst_sine_error(44_100, 48_000, hz);
            assert!(worst <= SINC_MAX_ERROR_LSB, "{hz} Hz: {worst} LSB");
        }
    }

    #[test]
    fn sinc_48000_to_44100_sine_error_is_within_the_bound() {
        for hz in ERROR_FREQUENCIES {
            let worst = worst_sine_error(48_000, 44_100, hz);
            assert!(worst <= SINC_MAX_ERROR_LSB, "{hz} Hz: {worst} LSB");
        }
    }

    #[test]
    fn linear_interpolation_is_measurably_worse_than_sinc() {
        let input = precise_sine(44_100, 8_000, 4_410);
        let linear = resample_linear(&input, 1, 44_100, 48_000).expect("linear");
        let sinc = resample_sinc(&input, 1, 44_100, 48_000).expect("sinc");
        let expected = precise_sine(48_000, 8_000, sinc.len());
        let edge = 64;
        let mut linear_worst = 0;
        let mut sinc_worst = 0;
        for ((linear_sample, sinc_sample), reference) in linear[edge..linear.len() - edge]
            .iter()
            .zip(sinc[edge..sinc.len() - edge].iter())
            .zip(expected[edge..expected.len() - edge].iter())
        {
            linear_worst =
                linear_worst.max((i32::from(*linear_sample) - i32::from(*reference)).abs());
            sinc_worst = sinc_worst.max((i32::from(*sinc_sample) - i32::from(*reference)).abs());
        }
        assert!(
            linear_worst > 10 * sinc_worst,
            "linear {linear_worst}, sinc {sinc_worst}"
        );
    }

    #[test]
    fn sinc_does_not_overflow_on_full_scale_input() {
        let mut input = vec![i16::MAX; 512];
        input[256] = i16::MIN;
        input[257] = i16::MAX;
        let output = resample_sinc(&input, 1, 48_000, 44_100).expect("resample");
        assert_eq!(output.len(), output_frames(512, 48_000, 44_100).unwrap());
        let peak = output
            .iter()
            .map(|sample| i32::from(*sample).abs())
            .max()
            .expect("non-empty output");
        assert!(peak > 30_000, "peak {peak} looks like wrap-around");
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(resample_linear(&[0; 8], 0, 48_000, 44_100).is_none());
        assert!(resample_linear(&[0; 8], 9, 48_000, 44_100).is_none());
        assert!(resample_linear(&[0; 8], 1, 0, 44_100).is_none());
        assert!(resample_linear(&[0; 8], 1, 48_000, 0).is_none());
        assert!(resample_sinc(&[0; 8], 0, 48_000, 44_100).is_none());
        assert!(resample_sinc(&[0; 8], 1, 48_000, 0).is_none());
        assert!(SincResampler::new(48_000, 44_100, 0).is_none());
    }

    #[test]
    fn empty_input_produces_empty_output() {
        assert_eq!(resample_linear(&[], 2, 44_100, 48_000), Some(Vec::new()));
        assert_eq!(resample_sinc(&[], 2, 44_100, 48_000), Some(Vec::new()));
    }

    #[test]
    fn accessors_report_the_configuration() {
        let resampler = SincResampler::new(44_100, 48_000, 2).expect("resampler");
        assert_eq!(resampler.from_rate(), 44_100);
        assert_eq!(resampler.to_rate(), 48_000);
        assert_eq!(resampler.channels(), 2);
        assert_eq!(resampler.output_frames(441), Some(480));
    }

    #[test]
    fn coefficient_rows_have_unity_dc_gain() {
        let table = build_coefficients(44_100, 48_000).expect("coefficients");
        assert_eq!(table.len(), SINC_PHASES * SINC_TAPS);
        for phase in 0..SINC_PHASES {
            let row = &table[phase * SINC_TAPS..(phase + 1) * SINC_TAPS];
            let sum: i32 = row.iter().map(|value| i32::from(*value)).sum();
            assert_eq!(sum, 32_768, "phase {phase}");
            assert_eq!(row[0], 0);
            assert_eq!(row[SINC_TAPS - 1], 0);
        }
    }
}
