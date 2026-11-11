// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integer-only audio core for RavenBSD: PCM mixing, format conversion, resampling, ring-buffer
//! arithmetic, WAV files and tone generation.
//!
//! This is the single place for the audio math that `audiod`, the sound drivers and the `play`
//! tool would otherwise duplicate (`tickets/next-backlogs/001-next-backlog-hw-complete.md`
//! section 5.3, B53). The crate is `no_std` + `alloc`, uses integer arithmetic only (DR-14),
//! forbids `unsafe`, and is deterministic: the same input always produces the same bytes.
//!
//! * [`format`] - the [`Format`] record and mono/stereo/upmix/downmix conversion.
//! * [`mix`] - saturating N-way mixing with Q15 per-stream gain.
//! * [`ramp`] - click-free linear volume ramps.
//! * [`resample`] - rational linear interpolation and a polyphase windowed-sinc quality mode.
//! * [`ring`] - frames/bytes/wrap arithmetic for the shared-memory PCM ring.
//! * [`wav`] - a strict RIFF PCM16 parser and a canonical writer.
//! * [`tone`] - table-based sine, square and silence for tests and self-tests.
//!
//! Every sample buffer is interleaved PCM16. The parser and the byte-level helpers never panic
//! on outside input; invalid data comes back as a typed error or `None`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod fixed;
#[cfg(test)]
mod test_rng;

pub mod format;
pub mod mix;
pub mod ramp;
pub mod resample;
pub mod ring;
pub mod tone;
pub mod wav;

pub use format::{
    ConvertError, Format, FormatError, MAX_CHANNELS, MAX_RATE, MIN_RATE, convert, downmix_mono,
    mono_to_stereo, stereo_to_mono, upmix_mono,
};
pub use mix::{GAIN_UNITY, Stream, mix};
pub use ramp::{envelope, ramp};
pub use resample::{
    SINC_MAX_ERROR_LSB, SINC_PHASES, SINC_TAPS, SincResampler, output_frames, resample_linear,
    resample_sinc,
};
pub use ring::{
    advance, bytes_remainder, bytes_to_frames, contiguous, frame_bytes, frames_to_bytes,
    free_bytes, used_bytes,
};
pub use tone::{SINE_LEN, SINE_Q15, Tone, Waveform};
pub use wav::{Wav, WavError, encode as encode_wav, parse as parse_wav};
