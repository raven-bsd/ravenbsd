// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PCM16 stream formats and interleaved channel conversion.
//!
//! Every buffer in this crate is interleaved signed 16-bit little-endian PCM. In memory an
//! `i16` is already the host's native representation; the `LE` part of the name only matters
//! when the samples are written to a byte stream (`raven-proto-audio`, WAV). The helpers here
//! copy between layouts of the same sample rate: mono, stereo and up to
//! [`MAX_CHANNELS`] channels.

use core::fmt;

/// Smallest sample rate accepted by a [`Format`].
pub const MIN_RATE: u32 = 8_000;
/// Largest sample rate accepted by a [`Format`].
pub const MAX_RATE: u32 = 192_000;
/// Largest channel count accepted by this crate.
pub const MAX_CHANNELS: u8 = 8;

/// A PCM16 stream format: sample rate and interleaved channel count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    /// Sample rate in Hz, `8000..=192000`.
    pub rate: u32,
    /// Interleaved channel count, `1..=8`.
    pub channels: u8,
}

impl Format {
    /// CD-quality stereo, the common case.
    pub const CD: Self = Self {
        rate: 44_100,
        channels: 2,
    };

    /// Validate the rate and channel count.
    pub const fn check(&self) -> Result<(), FormatError> {
        if self.rate < MIN_RATE || self.rate > MAX_RATE {
            return Err(FormatError::Rate);
        }
        if self.channels == 0 || self.channels > MAX_CHANNELS {
            return Err(FormatError::Channels);
        }
        Ok(())
    }

    /// Bytes in one interleaved frame (one sample of every channel).
    pub const fn frame_bytes(&self) -> usize {
        self.channels as usize * 2
    }
}

/// Why a [`Format`] was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// The sample rate is outside `8000..=192000`.
    Rate,
    /// The channel count is outside `1..=8`.
    Channels,
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Rate => "sample rate is outside 8000..=192000 Hz",
            Self::Channels => "channel count is outside 1..=8",
        })
    }
}

/// Why a channel conversion was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvertError {
    /// One of the two channel counts is zero or larger than [`MAX_CHANNELS`].
    Channels,
    /// Both layouts have more than one channel but different counts; there is no defined
    /// mapping between them without an explicit speaker layout.
    UnsupportedMapping {
        /// Source channel count.
        from: u8,
        /// Destination channel count.
        to: u8,
    },
}

impl fmt::Display for ConvertError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Channels => formatter.write_str("channel count is outside 1..=8"),
            Self::UnsupportedMapping { from, to } => {
                write!(formatter, "no channel mapping from {from} to {to} channels")
            }
        }
    }
}

/// Copy mono frames into `channels` interleaved channels.
///
/// Returns the number of frames written: the shorter of the input and what fits in `out`.
/// An invalid channel count writes nothing and returns zero.
pub fn upmix_mono(input: &[i16], channels: u8, out: &mut [i16]) -> usize {
    if channels == 0 || channels > MAX_CHANNELS {
        return 0;
    }
    let channels = usize::from(channels);
    let frames = input.len().min(out.len() / channels);
    for (frame, value) in input.iter().take(frames).enumerate() {
        let base = frame * channels;
        for slot in &mut out[base..base + channels] {
            *slot = *value;
        }
    }
    frames
}

/// Average `channels` interleaved channels down to one mono channel.
///
/// The sum is divided by the channel count with truncation towards zero, which keeps a silent
/// input silent. Returns the number of frames written. An invalid channel count writes nothing
/// and returns zero.
pub fn downmix_mono(input: &[i16], channels: u8, out: &mut [i16]) -> usize {
    if channels == 0 || channels > MAX_CHANNELS {
        return 0;
    }
    let channels = usize::from(channels);
    let frames = (input.len() / channels).min(out.len());
    for (frame, slot) in out.iter_mut().take(frames).enumerate() {
        let base = frame * channels;
        let sum: i32 = input[base..base + channels]
            .iter()
            .map(|sample| i32::from(*sample))
            .sum();
        *slot = (sum / channels as i32) as i16;
    }
    frames
}

/// Copy or convert between interleaved layouts.
///
/// Equal channel counts are a plain copy, mono to `N` duplicates the sample and `N` to mono
/// averages all channels. Any other mapping is rejected with
/// [`ConvertError::UnsupportedMapping`] because PCM16 carries no speaker layout.
pub fn convert(input: &[i16], from: u8, to: u8, out: &mut [i16]) -> Result<usize, ConvertError> {
    if !(1..=MAX_CHANNELS).contains(&from) || !(1..=MAX_CHANNELS).contains(&to) {
        return Err(ConvertError::Channels);
    }
    if from == to {
        let frames = input.len().min(out.len());
        out[..frames].copy_from_slice(&input[..frames]);
        return Ok(frames);
    }
    if from == 1 {
        return Ok(upmix_mono(input, to, out));
    }
    if to == 1 {
        return Ok(downmix_mono(input, from, out));
    }
    Err(ConvertError::UnsupportedMapping { from, to })
}

/// Copy mono samples into a stereo buffer, duplicating each sample.
pub fn mono_to_stereo(input: &[i16], out: &mut [i16]) -> usize {
    upmix_mono(input, 2, out)
}

/// Average a stereo buffer into mono.
pub fn stereo_to_mono(input: &[i16], out: &mut [i16]) -> usize {
    downmix_mono(input, 2, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cd_format_is_valid() {
        assert_eq!(Format::CD.check(), Ok(()));
        assert_eq!(Format::CD.frame_bytes(), 4);
    }

    #[test]
    fn format_bounds_are_enforced() {
        for rate in [0, 1, 7_999, 192_001, u32::MAX] {
            let format = Format { rate, channels: 2 };
            assert_eq!(format.check(), Err(FormatError::Rate), "rate {rate}");
        }
        for channels in [0, 9, 255] {
            let format = Format {
                rate: 48_000,
                channels,
            };
            assert_eq!(
                format.check(),
                Err(FormatError::Channels),
                "channels {channels}"
            );
        }
        for channels in 1..=MAX_CHANNELS {
            let format = Format {
                rate: 48_000,
                channels,
            };
            assert_eq!(format.check(), Ok(()), "channels {channels}");
            assert_eq!(format.frame_bytes(), usize::from(channels) * 2);
        }
    }

    #[test]
    fn error_text_is_human_readable() {
        assert!(FormatError::Rate.to_string().contains("8000"));
        assert!(FormatError::Channels.to_string().contains("1..=8"));
        assert!(
            ConvertError::UnsupportedMapping { from: 2, to: 6 }
                .to_string()
                .contains("2 to 6")
        );
        assert!(ConvertError::Channels.to_string().contains("1..=8"));
    }

    #[test]
    fn mono_upmix_duplicates_every_sample() {
        let input = [1i16, -2, 3];
        let mut out = [0i16; 6];
        assert_eq!(upmix_mono(&input, 2, &mut out), 3);
        assert_eq!(out, [1, 1, -2, -2, 3, 3]);
    }

    #[test]
    fn upmix_to_eight_channels_repeats_the_sample() {
        let input = [7i16, -7];
        let mut out = [0i16; 16];
        assert_eq!(upmix_mono(&input, 8, &mut out), 2);
        for frame in 0..2 {
            for channel in 0..8 {
                assert_eq!(out[frame * 8 + channel], input[frame]);
            }
        }
    }

    #[test]
    fn downmix_averages_and_truncates() {
        let input = [100i16, 200, -100, -200, 1, 2];
        let mut out = [0i16; 3];
        assert_eq!(downmix_mono(&input, 2, &mut out), 3);
        assert_eq!(out, [150, -150, 1]);
    }

    #[test]
    fn downmix_keeps_silence_silent() {
        let input = [0i16; 16];
        let mut out = [-1i16; 4];
        assert_eq!(downmix_mono(&input, 4, &mut out), 4);
        assert_eq!(out, [0, 0, 0, 0]);
    }

    #[test]
    fn mono_and_stereo_helpers_agree_with_the_generic_ones() {
        let input = [5i16, 6];
        let mut stereo = [0i16; 4];
        assert_eq!(mono_to_stereo(&input, &mut stereo), 2);
        let mut again = [0i16; 4];
        assert_eq!(upmix_mono(&input, 2, &mut again), 2);
        assert_eq!(stereo, again);

        let mut mono = [0i16; 2];
        assert_eq!(stereo_to_mono(&stereo, &mut mono), 2);
        assert_eq!(mono, input);
    }

    #[test]
    fn convert_copies_equal_layouts_and_maps_mono() {
        let stereo = [1i16, 2, 3, 4];
        let mut same = [0i16; 4];
        assert_eq!(convert(&stereo, 2, 2, &mut same), Ok(4));
        assert_eq!(same, stereo);

        let mono = [1i16, 2];
        let mut up = [0i16; 8];
        assert_eq!(convert(&mono, 1, 4, &mut up), Ok(2));
        assert_eq!(up, [1, 1, 1, 1, 2, 2, 2, 2]);

        let quad = [1i16, 3, 5, 7];
        let mut down = [0i16; 4];
        assert_eq!(convert(&quad, 4, 1, &mut down), Ok(1));
        assert_eq!(down[0], 4);
    }

    #[test]
    fn convert_rejects_unsupported_layouts_and_bad_counts() {
        let input = [0i16; 8];
        let mut out = [0i16; 8];
        assert_eq!(
            convert(&input, 2, 6, &mut out),
            Err(ConvertError::UnsupportedMapping { from: 2, to: 6 })
        );
        assert_eq!(convert(&input, 0, 2, &mut out), Err(ConvertError::Channels));
        assert_eq!(convert(&input, 2, 9, &mut out), Err(ConvertError::Channels));
    }

    #[test]
    fn conversion_stops_at_the_shorter_buffer() {
        let input = [1i16, 2, 3, 4, 5, 6];
        let mut out = [0i16; 4];
        assert_eq!(upmix_mono(&input, 2, &mut out), 2);
        assert_eq!(out, [1, 1, 2, 2]);
        let mut tiny = [0i16; 1];
        assert_eq!(downmix_mono(&input, 2, &mut tiny), 1);
        assert_eq!(tiny, [1]);
    }
}
