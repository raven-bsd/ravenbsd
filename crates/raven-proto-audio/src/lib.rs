// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PCM stream protocol and driver ring layout for the `audio:` scheme.
//!
//! `audiod` mixes client streams and drives a sound device. Both sides need one versioned,
//! golden-tested contract before either exists (`architecture/03-system-servers.md` section 2,
//! `tickets/next-backlogs/001-next-backlog-hw-complete.md` section 5.3). This crate defines:
//!
//! * [`stream`] - `OPEN`, plain `WRITE` of PCM bytes and the `SET_FORMAT`, `SET_BUFFER`,
//!   `START`, `STOP`, `DRAIN`, `SET_VOLUME`, `GET_STATUS` operations on `audio:out<N>`;
//! * [`format`] - sample formats and the stream format record; `S16LE` is implemented,
//!   `S24LE`/`S32LE`/`F32LE` are reserved and rejected with `ENOTSUP`;
//! * [`path`] - parsing of `audio:out<N>`;
//! * [`ring`] - the shared-memory header and PCM frame ring between `audiod` and a driver, with
//!   single-producer/single-consumer helpers, period accounting and underrun detection.
//!
//! The crate is `no_std`, has no dependency except `raven-abi`, forbids `unsafe`, never allocates
//! in an encoder, never uses floating point and never panics while decoding. Every message starts
//! with a little-endian `u16` version. See `README.md` for the byte tables that match the golden
//! tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod format;
pub mod path;
pub mod ring;
pub mod stream;
pub mod wire;

use core::fmt;

pub use format::{ENOTSUP, Format, MAX_CHANNELS, MAX_RATE, MIN_RATE, SampleFormat};
pub use path::{AudioPath, MAX_DEVICE, PathError};
pub use raven_abi::errno::Errno;
pub use raven_abi::types::Handle;
pub use ring::{
    HEADER_LEN, LayoutError, MAX_FRAMES, MAX_PERIODS, MAX_REGION, RING_MAGIC, RING_VERSION,
    RingConsumer, RingError, RingHeader, RingLayout, RingProducer,
};
pub use stream::{
    BufferInfo, Call, MAX_PATH, MAX_WRITE, Status, StreamRequest, StreamResponse, StreamState,
};
pub use wire::{Reader, WireError, Writer};

/// Version of the audio stream and driver-ring framing.
pub const PROTOCOL_VERSION: u16 = stream::PROTOCOL_VERSION;

/// Why a message was rejected by a decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeErrorKind {
    /// The input ended before the message did.
    Truncated,
    /// The input continues after the message ended.
    TrailingBytes,
    /// The first field is a version this crate does not speak.
    UnsupportedVersion,
    /// An operation code, response tag or sample format is unknown.
    UnknownTag,
    /// A length or count is outside its documented bound.
    LengthOutOfRange,
    /// A field is well formed but violates an invariant (reserved bits, bad path, ...).
    Malformed,
}

/// A decoder failure. Decoders return this instead of panicking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError {
    kind: DecodeErrorKind,
}

impl DecodeError {
    /// Build an error of `kind`.
    pub const fn new(kind: DecodeErrorKind) -> Self {
        Self { kind }
    }

    /// Classify the failure.
    pub const fn kind(self) -> DecodeErrorKind {
        self.kind
    }

    /// The input ended too early.
    pub const fn truncated() -> Self {
        Self::new(DecodeErrorKind::Truncated)
    }

    /// The input has bytes after the message.
    pub const fn trailing() -> Self {
        Self::new(DecodeErrorKind::TrailingBytes)
    }

    /// The message uses another protocol version.
    pub const fn version() -> Self {
        Self::new(DecodeErrorKind::UnsupportedVersion)
    }

    /// An operation, tag or format code is unknown.
    pub const fn tag() -> Self {
        Self::new(DecodeErrorKind::UnknownTag)
    }

    /// A length or count is out of range.
    pub const fn length() -> Self {
        Self::new(DecodeErrorKind::LengthOutOfRange)
    }

    /// A field violates a protocol invariant.
    pub const fn malformed() -> Self {
        Self::new(DecodeErrorKind::Malformed)
    }
}

impl From<WireError> for DecodeError {
    fn from(error: WireError) -> Self {
        match error {
            WireError::Truncated => Self::truncated(),
            WireError::TooLong => Self::length(),
            WireError::Invalid => Self::malformed(),
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            DecodeErrorKind::Truncated => "truncated message",
            DecodeErrorKind::TrailingBytes => "trailing bytes after the message",
            DecodeErrorKind::UnsupportedVersion => "unsupported protocol version",
            DecodeErrorKind::UnknownTag => "unknown operation, tag or format",
            DecodeErrorKind::LengthOutOfRange => "length or count out of range",
            DecodeErrorKind::Malformed => "malformed field",
        })
    }
}
