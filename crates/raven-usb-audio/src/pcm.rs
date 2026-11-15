// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The PCM source seam between a sound server ring and the isochronous stream.
//!
//! The audiod driver ring is defined by `raven-proto-audio`: an 80-byte header, a 64-byte aligned
//! PCM area, frame indices and single-producer/single-consumer helpers. [`PcmSource`] is the
//! frame-aligned read interface the USB host consumes; it is implemented for the protocol's
//! [`RingConsumer`] (the driver side) and for a plain in-memory [`SliceSource`] used by tests and
//! simple callers.

use raven_proto_audio::ring::RingConsumer;

/// A frame-aligned PCM source.
pub trait PcmSource {
    /// Bytes per PCM frame.
    fn frame_bytes(&self) -> u16;

    /// Whole frames waiting to be read.
    fn available_frames(&self) -> u32;

    /// Read up to `out.len()` bytes (rounded down to whole frames); returns the bytes read.
    fn read(&mut self, out: &mut [u8]) -> usize;
}

/// An in-memory PCM source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliceSource<'a> {
    data: &'a [u8],
    position: usize,
    frame_bytes: u16,
}

impl<'a> SliceSource<'a> {
    /// Wrap a frame-aligned slice.
    ///
    /// Returns `None` for a zero frame size or a slice that is not a whole number of frames.
    #[must_use]
    pub const fn new(data: &'a [u8], frame_bytes: u16) -> Option<Self> {
        if frame_bytes == 0 || !data.len().is_multiple_of(frame_bytes as usize) {
            return None;
        }
        Some(Self {
            data,
            position: 0,
            frame_bytes,
        })
    }

    /// Bytes consumed so far.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Bytes left.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }
}

impl PcmSource for SliceSource<'_> {
    fn frame_bytes(&self) -> u16 {
        self.frame_bytes
    }

    fn available_frames(&self) -> u32 {
        (self.remaining() / usize::from(self.frame_bytes)) as u32
    }

    fn read(&mut self, out: &mut [u8]) -> usize {
        let frame = usize::from(self.frame_bytes);
        let want = (out.len().min(self.remaining()) / frame) * frame;
        let end = self.position.saturating_add(want);
        match (self.data.get(self.position..end), out.get_mut(..want)) {
            (Some(source), Some(destination)) => {
                destination.copy_from_slice(source);
                self.position = end;
                want
            }
            _ => 0,
        }
    }
}

impl PcmSource for RingConsumer<'_> {
    fn frame_bytes(&self) -> u16 {
        self.layout().frame_bytes()
    }

    fn available_frames(&self) -> u32 {
        RingConsumer::available_frames(self)
    }

    fn read(&mut self, out: &mut [u8]) -> usize {
        let frame = usize::from(self.layout().frame_bytes());
        RingConsumer::read(self, out)
            .map(|frames| frames.saturating_mul(frame))
            .unwrap_or(0)
    }
}

#[cfg(test)]
#[path = "pcm_ut.rs"]
mod tests;
