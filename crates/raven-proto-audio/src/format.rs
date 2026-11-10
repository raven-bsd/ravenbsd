// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PCM sample formats and stream format records.
//!
//! `S16LE` is the only format implemented in phase 1. `S24LE`, `S32LE` and `F32LE` are reserved
//! on the wire so a client can request them and receive `ENOTSUP`; they must not be silently
//! accepted. All arithmetic here is integer-only (DR-14).

use core::fmt;

use raven_abi::errno::Errno;

/// `ENOTSUP` value used by the audio protocol. `raven-abi` names the same number
/// `EOPNOTSUPP` (95); both spellings are the Linux `ENOTSUP`.
pub const ENOTSUP: Errno = Errno::EOPNOTSUPP;

/// Smallest accepted sample rate in Hz.
pub const MIN_RATE: u32 = 8_000;
/// Largest accepted sample rate in Hz.
pub const MAX_RATE: u32 = 192_000;
/// Largest accepted channel count.
pub const MAX_CHANNELS: u8 = 8;

/// PCM sample encoding carried by a stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SampleFormat {
    /// Signed 16-bit little-endian. The only format implemented in phase 1.
    S16Le = 1,
    /// Signed 24-bit little-endian, packed in three bytes. Reserved.
    S24Le = 2,
    /// Signed 32-bit little-endian. Reserved.
    S32Le = 3,
    /// IEEE-754 binary32 little-endian. Reserved; the crate itself never uses floating point.
    F32Le = 4,
}

impl SampleFormat {
    /// Wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decode a wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::S16Le),
            2 => Some(Self::S24Le),
            3 => Some(Self::S32Le),
            4 => Some(Self::F32Le),
            _ => None,
        }
    }

    /// Whether phase 1 can play this format.
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::S16Le)
    }

    /// Bytes per sample, including padding for packed 24-bit samples.
    pub const fn bytes_per_sample(self) -> u8 {
        match self {
            Self::S16Le => 2,
            Self::S24Le => 3,
            Self::S32Le | Self::F32Le => 4,
        }
    }
}

impl fmt::Display for SampleFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::S16Le => "S16LE",
            Self::S24Le => "S24LE",
            Self::S32Le => "S32LE",
            Self::F32Le => "F32LE",
        })
    }
}

/// A stream format record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    /// Sample rate in Hz, `8000..=192000`.
    pub rate: u32,
    /// Channel count, `1..=8`.
    pub channels: u8,
    /// Sample encoding.
    pub sample: SampleFormat,
}

impl Format {
    /// CD-quality stereo, the common case.
    pub const CD: Self = Self {
        rate: 44_100,
        channels: 2,
        sample: SampleFormat::S16Le,
    };

    /// Check the numeric bounds (rate and channel count) without checking support.
    ///
    /// Decoders use this: a reserved format is still a well-formed request and must reach the
    /// server, which answers `ENOTSUP`, rather than becoming a framing error.
    pub fn check_bounds(&self) -> Result<(), Errno> {
        if !(MIN_RATE..=MAX_RATE).contains(&self.rate) {
            return Err(Errno::EINVAL);
        }
        if self.channels == 0 || self.channels > MAX_CHANNELS {
            return Err(Errno::EINVAL);
        }
        Ok(())
    }

    /// Check everything needed to play this format: bounds and support.
    ///
    /// Returns `EINVAL` for a rate or channel count outside the documented range and `ENOTSUP`
    /// for a reserved sample format.
    pub fn check(&self) -> Result<(), Errno> {
        self.check_bounds()?;
        if !self.sample.is_supported() {
            return Err(ENOTSUP);
        }
        Ok(())
    }

    /// Bytes in one frame (all channels of one sample period).
    pub const fn bytes_per_frame(&self) -> u16 {
        // 255 channels * 4 bytes/sample = 1020, so this cannot overflow a u16.
        self.channels as u16 * self.sample.bytes_per_sample() as u16
    }

    /// Whether phase 1 can play this format.
    pub const fn is_supported(&self) -> bool {
        self.sample.is_supported()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s16_frame_sizes_are_exact() {
        let mono = Format {
            rate: 8_000,
            channels: 1,
            sample: SampleFormat::S16Le,
        };
        let stereo = Format {
            rate: 48_000,
            channels: 2,
            sample: SampleFormat::S16Le,
        };
        assert_eq!(mono.bytes_per_frame(), 2);
        assert_eq!(stereo.bytes_per_frame(), 4);
        assert_eq!(Format::CD.bytes_per_frame(), 4);
    }

    #[test]
    fn reserved_formats_are_rejected_with_enotsup() {
        for sample in [
            SampleFormat::S24Le,
            SampleFormat::S32Le,
            SampleFormat::F32Le,
        ] {
            let format = Format {
                rate: 48_000,
                channels: 2,
                sample,
            };
            assert_eq!(format.check(), Err(ENOTSUP), "{sample}");
            assert_eq!(format.check_bounds(), Ok(()));
            assert!(!format.is_supported());
        }
        assert_eq!(Format::CD.check(), Ok(()));
    }

    #[test]
    fn rate_and_channel_bounds_are_enforced() {
        let base = Format::CD;
        for rate in [0, 7_999, 192_001, u32::MAX] {
            let format = Format { rate, ..base };
            assert_eq!(format.check(), Err(Errno::EINVAL), "rate {rate}");
            assert_eq!(format.check_bounds(), Err(Errno::EINVAL));
        }
        for channels in [0, 9, 255] {
            let format = Format { channels, ..base };
            assert_eq!(format.check(), Err(Errno::EINVAL), "channels {channels}");
        }
        for rate in [8_000, 44_100, 48_000, 192_000] {
            assert_eq!(Format { rate, ..base }.check(), Ok(()), "rate {rate}");
        }
        for channels in 1..=8 {
            assert_eq!(Format { channels, ..base }.check(), Ok(()));
        }
    }

    #[test]
    fn sample_format_codes_roundtrip() {
        for sample in [
            SampleFormat::S16Le,
            SampleFormat::S24Le,
            SampleFormat::S32Le,
            SampleFormat::F32Le,
        ] {
            assert_eq!(SampleFormat::from_code(sample.code()), Some(sample));
            assert!(sample.bytes_per_sample() >= 2);
        }
        assert_eq!(SampleFormat::from_code(0), None);
        assert_eq!(SampleFormat::from_code(5), None);
    }

    #[test]
    fn bytes_per_frame_covers_eight_channels() {
        let format = Format {
            rate: 96_000,
            channels: MAX_CHANNELS,
            sample: SampleFormat::S32Le,
        };
        assert_eq!(format.bytes_per_frame(), 32);
    }

    #[test]
    fn display_uses_the_wire_names() {
        assert_eq!(SampleFormat::S16Le.to_string(), "S16LE");
        assert_eq!(SampleFormat::F32Le.to_string(), "F32LE");
    }
}
