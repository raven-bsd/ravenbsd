// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Isochronous PCM streaming and the UAC1 feedback value.
//!
//! A USB audio endpoint sends one packet per microframe (high speed, 8000/s) or per frame (full
//! speed, 1000/s). The packet carries a whole number of PCM frames; the schedule distributes the
//! fractional part deterministically so the long-term rate equals the requested rate. The
//! asynchronous feedback value is the samples-per-microframe ratio in 16.16 fixed point, the
//! UAC1 high-speed format. The device's real packet sizes and feedback behaviour are UNVERIFIED.

use core::fmt;

/// High-speed microframes per second.
pub const HIGH_SPEED_MICROFRAMES: u32 = 8000;
/// Full-speed frames per second.
pub const FULL_SPEED_FRAMES: u32 = 1000;
/// Largest channel count the model accepts.
pub const MAX_CHANNELS: u8 = 8;
/// Lowest sample rate the model accepts.
pub const MIN_RATE: u32 = 8_000;
/// Highest sample rate the model accepts.
pub const MAX_RATE: u32 = 192_000;

/// Why a stream parameter or transition is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamError {
    /// A PCM field is outside its range.
    BadFormat(&'static str),
    /// The microframe/frame rate is zero.
    BadFrameRate(u32),
    /// The endpoint packet size is zero.
    ZeroMaxPacket,
    /// The packet is larger than the endpoint's `wMaxPacketSize`.
    PacketTooLarge {
        /// Packet bytes.
        bytes: u32,
        /// Endpoint maximum.
        max: u16,
    },
    /// The stream was advanced before `start`.
    NotRunning,
    /// The packet index would wrap.
    IndexOverflow,
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadFormat(what) => write!(f, "invalid PCM format: {what}"),
            Self::BadFrameRate(rate) => write!(f, "invalid microframe rate {rate}"),
            Self::ZeroMaxPacket => f.write_str("endpoint max packet size is zero"),
            Self::PacketTooLarge { bytes, max } => {
                write!(f, "packet of {bytes} bytes exceeds {max}")
            }
            Self::NotRunning => f.write_str("stream is not running"),
            Self::IndexOverflow => f.write_str("packet index overflow"),
        }
    }
}

impl core::error::Error for StreamError {}

/// One PCM format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmFormat {
    /// Interleaved channels.
    pub channels: u8,
    /// Container bytes per sample (2 for S16, 3 for S24 in 4-byte slots).
    pub subframe_bytes: u8,
    /// Significant bits per sample.
    pub bit_resolution: u8,
    /// Sample rate in Hz.
    pub rate_hz: u32,
}

impl PcmFormat {
    /// Validate the format.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::BadFormat`] for a channel/subframe/bit/rate outside the model.
    pub const fn validate(&self) -> Result<(), StreamError> {
        if self.channels == 0 || self.channels > MAX_CHANNELS {
            return Err(StreamError::BadFormat("channel count"));
        }
        if self.subframe_bytes == 0 || self.subframe_bytes > 4 {
            return Err(StreamError::BadFormat("subframe size"));
        }
        if self.bit_resolution == 0 || self.bit_resolution > self.subframe_bytes.saturating_mul(8) {
            return Err(StreamError::BadFormat("bit resolution"));
        }
        if self.rate_hz < MIN_RATE || self.rate_hz > MAX_RATE {
            return Err(StreamError::BadFormat("sample rate"));
        }
        Ok(())
    }

    /// Bytes per PCM frame.
    #[must_use]
    pub const fn frame_bytes(&self) -> u32 {
        (self.channels as u32).saturating_mul(self.subframe_bytes as u32)
    }
}

/// The asynchronous feedback value in 16.16 samples per microframe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Feedback {
    /// Fixed-point value (high 16 bits integer, low 16 bits fraction).
    pub value: u32,
}

impl Feedback {
    /// Compute the feedback ratio for `rate_hz` over `frames_per_second` frames.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::BadFrameRate`] for a zero frame rate or a value that does not fit
    /// 16.16.
    pub fn from_rate(rate_hz: u32, frames_per_second: u32) -> Result<Self, StreamError> {
        if frames_per_second == 0 {
            return Err(StreamError::BadFrameRate(0));
        }
        let value = (u64::from(rate_hz) << 16) / u64::from(frames_per_second);
        let value = u32::try_from(value).map_err(|_| StreamError::BadFrameRate(rate_hz))?;
        Ok(Self { value })
    }

    /// The raw 16.16 value.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.value
    }

    /// The 4-byte little-endian wire form.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; 4] {
        self.value.to_le_bytes()
    }
}

/// One isochronous packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Packet {
    /// Packet index (one per microframe/frame).
    pub index: u32,
    /// PCM frames in this packet.
    pub frames: u32,
    /// Payload bytes.
    pub bytes: u32,
}

/// The packet cadence for one format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    format: PcmFormat,
    frames_per_second: u32,
    max_packet: u16,
}

impl Schedule {
    /// Build a schedule.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError`] for an invalid format, a zero frame rate or a zero packet size.
    pub fn new(
        format: PcmFormat,
        frames_per_second: u32,
        max_packet: u16,
    ) -> Result<Self, StreamError> {
        format.validate()?;
        if frames_per_second == 0 {
            return Err(StreamError::BadFrameRate(0));
        }
        if max_packet == 0 {
            return Err(StreamError::ZeroMaxPacket);
        }
        Ok(Self {
            format,
            frames_per_second,
            max_packet,
        })
    }

    /// The PCM format.
    #[must_use]
    pub const fn format(&self) -> PcmFormat {
        self.format
    }

    /// Frames per second (microframes or full-speed frames).
    #[must_use]
    pub const fn frames_per_second(&self) -> u32 {
        self.frames_per_second
    }

    /// The endpoint packet limit.
    #[must_use]
    pub const fn max_packet(&self) -> u16 {
        self.max_packet
    }

    /// PCM frames in packet `index`, using the exact cumulative distribution.
    #[must_use]
    pub fn frames_in(&self, index: u32) -> u32 {
        let rate = u64::from(self.format.rate_hz);
        let frames = u64::from(self.frames_per_second);
        let start = rate.saturating_mul(u64::from(index)) / frames;
        let end = rate.saturating_mul(u64::from(index).saturating_add(1)) / frames;
        u32::try_from(end.saturating_sub(start)).unwrap_or(u32::MAX)
    }

    /// Packet bytes for `index`.
    #[must_use]
    pub fn bytes_in(&self, index: u32) -> u32 {
        self.frames_in(index)
            .saturating_mul(self.format.frame_bytes())
    }

    /// The packet at `index`, checked against the endpoint limit.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::PacketTooLarge`] when the payload exceeds the endpoint maximum.
    pub fn packet(&self, index: u32) -> Result<Packet, StreamError> {
        let frames = self.frames_in(index);
        let bytes = frames.saturating_mul(self.format.frame_bytes());
        if bytes > u32::from(self.max_packet) {
            return Err(StreamError::PacketTooLarge {
                bytes,
                max: self.max_packet,
            });
        }
        Ok(Packet {
            index,
            frames,
            bytes,
        })
    }

    /// The feedback value for this schedule.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::BadFrameRate`] if the ratio does not fit 16.16.
    pub fn feedback(&self) -> Result<Feedback, StreamError> {
        Feedback::from_rate(self.format.rate_hz, self.frames_per_second)
    }
}

/// The packet producer for one endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoStream {
    schedule: Schedule,
    next: u32,
    running: bool,
}

impl IsoStream {
    /// A stopped stream at packet zero.
    #[must_use]
    pub const fn new(schedule: Schedule) -> Self {
        Self {
            schedule,
            next: 0,
            running: false,
        }
    }

    /// Start (or resume) the stream.
    pub fn start(&mut self) {
        self.running = true;
    }

    /// Stop the stream.
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// True when the stream is producing packets.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// The schedule.
    #[must_use]
    pub const fn schedule(&self) -> Schedule {
        self.schedule
    }

    /// The next packet index.
    #[must_use]
    pub const fn next_index(&self) -> u32 {
        self.next
    }

    /// Produce the next packet and advance.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::NotRunning`] before `start` and [`StreamError::IndexOverflow`] when
    /// the index cannot advance.
    pub fn next_packet(&mut self) -> Result<Packet, StreamError> {
        if !self.running {
            return Err(StreamError::NotRunning);
        }
        let packet = self.schedule.packet(self.next)?;
        self.next = self.next.checked_add(1).ok_or(StreamError::IndexOverflow)?;
        Ok(packet)
    }
}

#[cfg(test)]
#[path = "stream_ut.rs"]
mod tests;
