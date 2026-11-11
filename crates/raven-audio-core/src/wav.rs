// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Strict RIFF/WAVE PCM16 parser and writer.
//!
//! Accepted input is an uncompressed (`WAVE_FORMAT_PCM`, tag 1), 16-bit, mono..octal WAVE file
//! at 8 000..=192 000 Hz. Unusual but valid layouts are handled: `fmt ` and `data` in any
//! order, unknown chunks before or after them, odd-sized chunks with their RIFF pad byte, and
//! `fmt ` chunk sizes of 16, 18 or 40 bytes.
//!
//! Everything else is an error, including a chunk whose declared size runs past the end of the
//! buffer, a partial trailing frame, an odd or unknown `fmt ` size, and a reserved format tag
//! or sample width. The parser never panics: every offset is checked and every length is
//! validated before it is used.

use alloc::vec::Vec;
use core::fmt;

use crate::format::{Format, MAX_CHANNELS, MAX_RATE, MIN_RATE};

/// Format tags (`wFormatTag`) that this parser accepts.
const WAVE_FORMAT_PCM: u16 = 1;
/// `fmt ` chunk sizes that are defined by the RIFF specification.
const FMT_SIZES: [u32; 3] = [16, 18, 40];

/// A parsed PCM16 WAVE file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wav {
    /// Sample rate in Hz.
    pub rate: u32,
    /// Interleaved channel count.
    pub channels: u16,
    /// Interleaved signed 16-bit samples.
    pub samples: Vec<i16>,
}

impl Wav {
    /// Number of sample frames (samples per channel).
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / usize::from(self.channels)
        }
    }
}

/// Why a WAVE buffer was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WavError {
    /// The first four bytes are not `RIFF` or the buffer is shorter than a RIFF header.
    NotRiff,
    /// The bytes at offset 8 are not `WAVE`.
    NotWave,
    /// A chunk declares more bytes than the buffer holds.
    Truncated,
    /// No `fmt ` chunk was found.
    MissingFmt,
    /// No `data` chunk was found.
    MissingData,
    /// The `fmt ` chunk size is not 16, 18 or 40 bytes.
    BadFmtSize(u32),
    /// The format tag is not `WAVE_FORMAT_PCM` (1).
    UnsupportedFormatTag(u16),
    /// The sample width is not 16 bits.
    UnsupportedBits(u16),
    /// The channel count is zero or above [`MAX_CHANNELS`].
    UnsupportedChannels(u16),
    /// The sample rate is outside `8000..=192000`.
    UnsupportedRate(u32),
    /// `nBlockAlign` does not equal `channels * 2`.
    BadBlockAlign(u16),
    /// The `data` chunk does not hold a whole number of frames.
    PartialFrame,
    /// The buffer cannot be represented or allocated.
    TooLarge,
}

impl fmt::Display for WavError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRiff => formatter.write_str("not a RIFF file"),
            Self::NotWave => formatter.write_str("RIFF form is not WAVE"),
            Self::Truncated => formatter.write_str("a chunk is truncated"),
            Self::MissingFmt => formatter.write_str("no fmt chunk"),
            Self::MissingData => formatter.write_str("no data chunk"),
            Self::BadFmtSize(size) => {
                write!(formatter, "fmt chunk size {size} is not 16, 18 or 40")
            }
            Self::UnsupportedFormatTag(tag) => {
                write!(formatter, "format tag {tag} is not PCM (1)")
            }
            Self::UnsupportedBits(bits) => write!(formatter, "sample width {bits} is not 16 bits"),
            Self::UnsupportedChannels(channels) => {
                write!(
                    formatter,
                    "channel count {channels} is outside 1..={MAX_CHANNELS}"
                )
            }
            Self::UnsupportedRate(rate) => {
                write!(
                    formatter,
                    "sample rate {rate} is outside {MIN_RATE}..={MAX_RATE}"
                )
            }
            Self::BadBlockAlign(align) => write!(formatter, "block alignment {align} is wrong"),
            Self::PartialFrame => formatter.write_str("data chunk ends inside a frame"),
            Self::TooLarge => formatter.write_str("WAVE data is too large"),
        }
    }
}

struct Fmt {
    channels: u16,
    rate: u32,
    block_align: u16,
}

fn chunk_id(bytes: &[u8], offset: usize) -> [u8; 4] {
    [
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn parse_fmt(bytes: &[u8], declared: u32) -> Result<Fmt, WavError> {
    if !FMT_SIZES.contains(&declared) || bytes.len() < 16 {
        return Err(WavError::BadFmtSize(declared));
    }
    let tag = u16_at(bytes, 0);
    if tag != WAVE_FORMAT_PCM {
        return Err(WavError::UnsupportedFormatTag(tag));
    }
    let channels = u16_at(bytes, 2);
    if channels == 0 || channels > u16::from(MAX_CHANNELS) {
        return Err(WavError::UnsupportedChannels(channels));
    }
    let rate = u32_at(bytes, 4);
    if !(MIN_RATE..=MAX_RATE).contains(&rate) {
        return Err(WavError::UnsupportedRate(rate));
    }
    let block_align = u16_at(bytes, 12);
    let bits = u16_at(bytes, 14);
    if bits != 16 {
        return Err(WavError::UnsupportedBits(bits));
    }
    if block_align != channels * 2 {
        return Err(WavError::BadBlockAlign(block_align));
    }
    Ok(Fmt {
        channels,
        rate,
        block_align,
    })
}

/// Parse a RIFF/WAVE PCM16 buffer.
pub fn parse(bytes: &[u8]) -> Result<Wav, WavError> {
    if bytes.len() < 12 {
        return Err(WavError::NotRiff);
    }
    if &bytes[0..4] != b"RIFF" {
        return Err(WavError::NotRiff);
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotWave);
    }
    let mut position = 12usize;
    let mut fmt: Option<Fmt> = None;
    let mut data: Option<(usize, usize)> = None;
    while position + 8 <= bytes.len() {
        let id = chunk_id(bytes, position);
        let declared_u32 = u32_at(bytes, position + 4);
        let body = position + 8;
        let available = bytes.len() - body;
        if declared_u32 as usize > available {
            return Err(WavError::Truncated);
        }
        let declared = declared_u32 as usize;
        match &id {
            b"fmt " if fmt.is_none() => {
                fmt = Some(parse_fmt(&bytes[body..body + declared], declared_u32)?);
            }
            b"data" if data.is_none() => {
                data = Some((body, declared));
            }
            _ => {}
        }
        // Odd-sized chunks carry one pad byte that is not counted in the chunk size.
        let padded = declared
            .checked_add(declared & 1)
            .ok_or(WavError::Truncated)?;
        position = body.checked_add(padded).ok_or(WavError::Truncated)?;
    }
    let fmt = fmt.ok_or(WavError::MissingFmt)?;
    let (start, length) = data.ok_or(WavError::MissingData)?;
    let block_align = usize::from(fmt.block_align);
    if length % block_align != 0 {
        return Err(WavError::PartialFrame);
    }
    let count = length / 2;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(count)
        .map_err(|_| WavError::TooLarge)?;
    for index in 0..count {
        let offset = start + index * 2;
        samples.push(i16::from_le_bytes([bytes[offset], bytes[offset + 1]]));
    }
    Ok(Wav {
        rate: fmt.rate,
        channels: fmt.channels,
        samples,
    })
}

/// Encode a [`Wav`] as a canonical 44-byte-header PCM16 file.
///
/// Returns `None` for a rate or channel count that [`parse`] would reject, for a sample buffer
/// that is not a whole number of frames, and when the file would not fit in a RIFF 32-bit size
/// or in memory.
pub fn encode(wav: &Wav) -> Option<Vec<u8>> {
    let channels = u8::try_from(wav.channels).ok()?;
    Format {
        rate: wav.rate,
        channels,
    }
    .check()
    .ok()?;
    let frame = usize::from(wav.channels) * 2;
    if !wav.samples.len().is_multiple_of(usize::from(wav.channels)) {
        return None;
    }
    let data_bytes = wav.samples.len().checked_mul(2)?;
    if data_bytes > u32::MAX as usize {
        return None;
    }
    // "WAVE" + fmt chunk (8 + 16) + data chunk header (8).
    let riff_size = 4 + 24 + 8 + data_bytes;
    if riff_size > u32::MAX as usize {
        return None;
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(riff_size + 8).ok()?;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(riff_size as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
    bytes.extend_from_slice(&wav.channels.to_le_bytes());
    bytes.extend_from_slice(&wav.rate.to_le_bytes());
    let byte_rate = wav.rate.checked_mul(u32::from(frame as u16))?;
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&(frame as u16).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data_bytes as u32).to_le_bytes());
    for sample in &wav.samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rng::Rng;
    use alloc::vec;

    fn sample_wav() -> Wav {
        Wav {
            rate: 48_000,
            channels: 2,
            samples: vec![0, 1, -1, 32_767, -32_768, 42, -42, 7],
        }
    }

    /// Append `id` and `body` with the RIFF pad byte when the body is odd.
    fn push_chunk(bytes: &mut Vec<u8>, id: &[u8; 4], body: &[u8]) {
        bytes.extend_from_slice(id);
        bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(body);
        if body.len() % 2 == 1 {
            bytes.push(0);
        }
    }

    fn fmt_body(channels: u16, rate: u32, bits: u16) -> Vec<u8> {
        let block = channels * (bits / 8);
        let mut body = Vec::new();
        body.extend_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
        body.extend_from_slice(&channels.to_le_bytes());
        body.extend_from_slice(&rate.to_le_bytes());
        body.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
        body.extend_from_slice(&block.to_le_bytes());
        body.extend_from_slice(&bits.to_le_bytes());
        body
    }

    /// Build a file with an explicit `fmt ` body and data body plus surrounding chunks.
    fn build(
        fmt: &[u8],
        data: &[u8],
        before_fmt: &[(&[u8; 4], &[u8])],
        after_data: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        for (id, body) in before_fmt {
            push_chunk(&mut bytes, id, body);
        }
        push_chunk(&mut bytes, b"fmt ", fmt);
        push_chunk(&mut bytes, b"data", data);
        for (id, body) in after_data {
            push_chunk(&mut bytes, id, body);
        }
        let size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        bytes
    }

    fn valid_bytes() -> Vec<u8> {
        encode(&sample_wav()).expect("encode")
    }

    #[test]
    fn round_trip_keeps_every_field() {
        let wav = sample_wav();
        let bytes = encode(&wav).expect("encode");
        assert_eq!(bytes.len(), 44 + wav.samples.len() * 2);
        assert_eq!(parse(&bytes), Ok(wav));
    }

    #[test]
    fn round_trip_holds_for_random_files() {
        let mut rng = Rng::new(0x7A71);
        let rates = [8_000u32, 11_025, 22_050, 44_100, 48_000, 96_000, 192_000];
        for _ in 0..500 {
            let channels = 1 + rng.below(4) as u16;
            let frames = rng.below(50) as usize;
            let mut samples = Vec::with_capacity(frames * usize::from(channels));
            for _ in 0..frames * usize::from(channels) {
                samples.push(rng.sample());
            }
            let wav = Wav {
                rate: rates[rng.below(rates.len() as u32) as usize],
                channels,
                samples,
            };
            let bytes = encode(&wav).expect("encode");
            assert_eq!(parse(&bytes), Ok(wav));
        }
    }

    #[test]
    fn a_hand_built_file_parses() {
        let data = [1i16, -1, 2, -2];
        let mut body = Vec::new();
        for sample in data {
            body.extend_from_slice(&sample.to_le_bytes());
        }
        let bytes = build(&fmt_body(2, 44_100, 16), &body, &[], &[]);
        let wav = parse(&bytes).expect("parse");
        assert_eq!(wav.rate, 44_100);
        assert_eq!(wav.channels, 2);
        assert_eq!(wav.samples, data);
        assert_eq!(wav.frames(), 2);
    }

    #[test]
    fn unknown_chunks_are_skipped() {
        let data = [5i16, -5];
        let mut body = Vec::new();
        for sample in data {
            body.extend_from_slice(&sample.to_le_bytes());
        }
        let bytes = build(
            &fmt_body(1, 48_000, 16),
            &body,
            &[(b"JUNK", b"hello")],
            &[(b"LIST", b"INFOIART\x00\x00\x00\x00")],
        );
        let wav = parse(&bytes).expect("parse");
        assert_eq!(wav.samples, data);
    }

    #[test]
    fn odd_sized_chunks_are_padded() {
        let bytes = build(
            &fmt_body(1, 8_000, 16),
            &[0, 0, 1, 0],
            &[(b"JUNK", b"odd")],
            &[(b"note", b"x")],
        );
        let wav = parse(&bytes).expect("parse");
        assert_eq!(wav.samples, [0, 1]);
    }

    #[test]
    fn data_before_fmt_is_accepted() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        push_chunk(&mut bytes, b"data", &[2, 0, 3, 0]);
        push_chunk(&mut bytes, b"fmt ", &fmt_body(1, 16_000, 16));
        let size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        let wav = parse(&bytes).expect("parse");
        assert_eq!(wav.rate, 16_000);
        assert_eq!(wav.samples, [2, 3]);
    }

    #[test]
    fn extended_fmt_sizes_are_accepted() {
        for extension in [2usize, 24] {
            let mut fmt = fmt_body(2, 48_000, 16);
            fmt.resize(16 + extension, 0);
            let bytes = build(&fmt, &[0; 8], &[], &[]);
            let wav = parse(&bytes).expect("parse");
            assert_eq!(wav.channels, 2);
        }
    }

    #[test]
    fn odd_fmt_size_is_rejected() {
        let mut fmt = fmt_body(1, 48_000, 16);
        fmt.push(0);
        let bytes = build(&fmt, &[0; 4], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::BadFmtSize(17)));
    }

    #[test]
    fn unknown_fmt_size_is_rejected() {
        let mut fmt = fmt_body(1, 48_000, 16);
        fmt.resize(20, 0);
        let bytes = build(&fmt, &[0; 4], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::BadFmtSize(20)));
    }

    #[test]
    fn wrong_format_tag_is_rejected() {
        let mut fmt = fmt_body(1, 48_000, 16);
        fmt[0..2].copy_from_slice(&3u16.to_le_bytes());
        let bytes = build(&fmt, &[0; 4], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::UnsupportedFormatTag(3)));
    }

    #[test]
    fn unsupported_sample_widths_are_rejected() {
        let bytes = build(&fmt_body(1, 48_000, 8), &[0; 4], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::UnsupportedBits(8)));
        let bytes = build(&fmt_body(1, 48_000, 24), &[0; 6], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::UnsupportedBits(24)));
    }

    #[test]
    fn channel_counts_are_validated() {
        let bytes = build(&fmt_body(0, 48_000, 16), &[], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::UnsupportedChannels(0)));
        let bytes = build(&fmt_body(9, 48_000, 16), &[0; 18], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::UnsupportedChannels(9)));
    }

    #[test]
    fn sample_rates_are_validated() {
        let bytes = build(&fmt_body(1, 7_999, 16), &[0; 2], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::UnsupportedRate(7_999)));
        let bytes = build(&fmt_body(1, 192_001, 16), &[0; 2], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::UnsupportedRate(192_001)));
    }

    #[test]
    fn bad_block_alignment_is_rejected() {
        let mut fmt = fmt_body(2, 48_000, 16);
        fmt[12..14].copy_from_slice(&2u16.to_le_bytes());
        let bytes = build(&fmt, &[0; 8], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::BadBlockAlign(2)));
    }

    #[test]
    fn truncated_headers_and_chunks_are_rejected() {
        assert_eq!(parse(&[]), Err(WavError::NotRiff));
        assert_eq!(parse(b"RIFF"), Err(WavError::NotRiff));
        assert_eq!(parse(b"RIFF\x00\x00\x00\x00AVI "), Err(WavError::NotWave));

        let mut bytes = valid_bytes();
        bytes.truncate(bytes.len() - 2);
        let declared = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
        assert!(declared as usize > bytes.len() - 44);
        assert_eq!(parse(&bytes), Err(WavError::Truncated));

        let mut bytes = valid_bytes();
        bytes.truncate(20);
        assert_eq!(parse(&bytes), Err(WavError::Truncated));
    }

    #[test]
    fn missing_chunks_are_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        push_chunk(&mut bytes, b"data", &[0; 4]);
        let size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        assert_eq!(parse(&bytes), Err(WavError::MissingFmt));

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        push_chunk(&mut bytes, b"fmt ", &fmt_body(1, 48_000, 16));
        let size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        assert_eq!(parse(&bytes), Err(WavError::MissingData));
    }

    #[test]
    fn a_partial_trailing_frame_is_rejected() {
        let bytes = build(&fmt_body(2, 48_000, 16), &[0; 6], &[], &[]);
        assert_eq!(parse(&bytes), Err(WavError::PartialFrame));
    }

    #[test]
    fn an_empty_data_chunk_is_valid() {
        let bytes = build(&fmt_body(1, 44_100, 16), &[], &[], &[]);
        let wav = parse(&bytes).expect("parse");
        assert!(wav.samples.is_empty());
        assert_eq!(wav.frames(), 0);
        let encoded = encode(&wav).expect("encode");
        assert_eq!(parse(&encoded), Ok(wav));
    }

    #[test]
    fn encode_rejects_invalid_buffers() {
        let mut wav = sample_wav();
        wav.rate = 1_000;
        assert!(encode(&wav).is_none());
        let mut wav = sample_wav();
        wav.channels = 0;
        assert!(encode(&wav).is_none());
        let mut wav = sample_wav();
        wav.channels = 9;
        assert!(encode(&wav).is_none());
        let mut wav = sample_wav();
        wav.samples.truncate(3);
        assert!(encode(&wav).is_none());
        let mut wav = sample_wav();
        wav.channels = 255;
        assert!(encode(&wav).is_none());
    }

    #[test]
    fn error_messages_are_specific() {
        assert!(WavError::NotRiff.to_string().contains("RIFF"));
        assert!(WavError::PartialFrame.to_string().contains("frame"));
        assert!(WavError::BadFmtSize(17).to_string().contains("17"));
        assert!(WavError::UnsupportedChannels(9).to_string().contains("9"));
        assert!(WavError::TooLarge.to_string().contains("too large"));
    }

    #[test]
    fn four_thousand_random_buffers_do_not_panic() {
        let mut rng = Rng::new(0xBAD5EED);
        for _ in 0..4_000 {
            let length = rng.below(160) as usize;
            let mut bytes = vec![0u8; length];
            rng.fill(&mut bytes);
            let _ = parse(&bytes);
        }
    }

    #[test]
    fn four_thousand_mutated_valid_buffers_do_not_panic() {
        let base = valid_bytes();
        let mut rng = Rng::new(0x0DDBA11);
        for _ in 0..4_000 {
            let mut bytes = base.clone();
            let mutations = 1 + rng.below(4);
            for _ in 0..mutations {
                let index = rng.below(bytes.len() as u32) as usize;
                bytes[index] = rng.next_u64() as u8;
            }
            let _ = parse(&bytes);
        }
    }
}
