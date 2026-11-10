// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `audio:` scheme paths.
//!
//! `audio:out<N>` names an output device (`out0` is the default sink). The number is the device
//! index in `audiod`'s registry, not a file descriptor; the stream handle is transferred by the
//! kernel on `OPEN`.

use core::fmt;

/// Largest accepted output device index.
pub const MAX_DEVICE: u8 = 31;

/// A parsed `audio:out<N>` path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioPath {
    /// Output device index, `0..=MAX_DEVICE`.
    pub device: u8,
}

/// Why an `audio:` path was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    /// The path does not start with `audio:`.
    NotAudio,
    /// The part after `audio:` is not `out<digits>`.
    MissingDevice,
    /// The device index is not a decimal number in `0..=MAX_DEVICE`.
    BadDevice,
}

impl fmt::Display for PathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotAudio => "not an audio: scheme path",
            Self::MissingDevice => "missing audio output device",
            Self::BadDevice => "invalid audio device index",
        })
    }
}

/// Parse an `audio:out<N>` path.
pub fn parse(path: &str) -> Result<AudioPath, PathError> {
    let rest = path.strip_prefix("audio:").ok_or(PathError::NotAudio)?;
    let digits = rest.strip_prefix("out").ok_or(PathError::MissingDevice)?;
    if digits.is_empty() || digits.len() > 2 {
        return Err(PathError::BadDevice);
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(PathError::BadDevice);
    }
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(PathError::BadDevice);
    }
    let device: u16 = digits.parse().map_err(|_| PathError::BadDevice)?;
    if device > u16::from(MAX_DEVICE) {
        return Err(PathError::BadDevice);
    }
    Ok(AudioPath {
        device: device as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_output_devices() {
        assert_eq!(parse("audio:out0"), Ok(AudioPath { device: 0 }));
        assert_eq!(parse("audio:out1"), Ok(AudioPath { device: 1 }));
        assert_eq!(parse("audio:out31"), Ok(AudioPath { device: 31 }));
    }

    #[test]
    fn rejects_foreign_schemes() {
        assert_eq!(parse("net:tcp/10.0.0.1:80"), Err(PathError::NotAudio));
        assert_eq!(parse(""), Err(PathError::NotAudio));
        assert_eq!(parse("audio:"), Err(PathError::MissingDevice));
    }

    #[test]
    fn rejects_bad_devices() {
        for path in [
            "audio:in0",
            "audio:out",
            "audio:out32",
            "audio:out00",
            "audio:outx",
            "audio:out1x",
            "audio:out123",
        ] {
            assert!(parse(path).is_err(), "{path} must be rejected");
        }
    }
}
