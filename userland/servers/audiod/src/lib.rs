// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable mixer and null sink for `audiod`.
//!
//! The server-side state is deliberately independent of the scheme and kernel: [`Mixer`] owns
//! the streams, their PCM buffers and the device format. The target `main` only translates S11
//! requests into calls on it. Every sample is integer PCM16; the device is 48 kHz stereo and
//! other rates are linearly resampled through `raven-audio-core`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod mixer;

pub use mixer::{
    DEVICE_CHANNELS, DEVICE_RATE, MAX_BUFFER_SECONDS, Mixer, PERIOD_FRAMES, Stream, StreamView,
};

#[cfg(test)]
#[path = "mixer_ut.rs"]
mod tests;
