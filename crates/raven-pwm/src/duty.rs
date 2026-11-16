// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S16LE to PWM duty conversion for the two-channel FIFO.
//!
//! With both channels enabled, one stereo S16LE frame becomes two 32-bit duty words. The PWM
//! range is the number of clock ticks in a frame; the signed sample is mapped around `RANGE/2`
//! and clamped to `0..=RANGE`.

/// Bytes in one interleaved S16LE stereo frame.
pub const STEREO_FRAME_BYTES: usize = 4;
/// Largest period the conversion helper accepts.
pub const MAX_PERIOD_FRAMES: usize = 4096;

/// Order of the two channels in the FIFO and in the jack wiring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleOrder {
    /// First word is the left channel, second is the right.
    LeftRight,
    /// First word is the right channel, second is the left (the B41 external default).
    RightLeft,
}

/// Why a PCM buffer cannot be converted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DutyError {
    /// The byte count is not a whole stereo S16LE frame.
    OddBytes,
    /// The destination cannot hold two words per frame.
    BufferTooSmall,
    /// The PWM range is zero.
    BadRange,
    /// The frame count exceeds [`MAX_PERIOD_FRAMES`].
    TooManyFrames,
}

/// Convert one signed sample to a duty count.
pub fn sample_to_duty(sample: i16, range: u32) -> Result<u32, DutyError> {
    if range == 0 {
        return Err(DutyError::BadRange);
    }
    let half = i64::from(range / 2);
    let duty = (i64::from(sample) * half) / 32762 + half;
    Ok(duty.clamp(0, i64::from(range)) as u32)
}

/// Convert one interleaved S16LE stereo period into duty words.
///
/// `out` must hold at least two words per frame. Returns the frame count.
pub fn convert_period(
    pcm: &[u8],
    range: u32,
    order: SampleOrder,
    out: &mut [u32],
) -> Result<usize, DutyError> {
    if range == 0 {
        return Err(DutyError::BadRange);
    }
    if !pcm.len().is_multiple_of(STEREO_FRAME_BYTES) {
        return Err(DutyError::OddBytes);
    }
    let frames = pcm.len() / STEREO_FRAME_BYTES;
    if frames > MAX_PERIOD_FRAMES {
        return Err(DutyError::TooManyFrames);
    }
    let words = frames.checked_mul(2).ok_or(DutyError::TooManyFrames)?;
    if out.len() < words {
        return Err(DutyError::BufferTooSmall);
    }
    for (frame, chunk) in pcm.chunks_exact(STEREO_FRAME_BYTES).enumerate() {
        let first = i16::from_le_bytes([chunk[0], chunk[1]]);
        let second = i16::from_le_bytes([chunk[2], chunk[3]]);
        let (left, right) = match order {
            SampleOrder::LeftRight => (first, second),
            SampleOrder::RightLeft => (second, first),
        };
        let slot = frame * 2;
        out[slot] = sample_to_duty(left, range)?;
        out[slot + 1] = sample_to_duty(right, range)?;
    }
    Ok(frames)
}

/// Fill a period with the AC-coupled midpoint (silence).
pub fn silence_period(range: u32, frames: usize, out: &mut [u32]) -> Result<usize, DutyError> {
    if range == 0 {
        return Err(DutyError::BadRange);
    }
    if frames > MAX_PERIOD_FRAMES {
        return Err(DutyError::TooManyFrames);
    }
    let words = frames.checked_mul(2).ok_or(DutyError::TooManyFrames)?;
    if out.len() < words {
        return Err(DutyError::BufferTooSmall);
    }
    out[..words].fill(range / 2);
    Ok(frames)
}

#[cfg(test)]
#[path = "duty_ut.rs"]
mod tests;
