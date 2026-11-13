// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Character-unit domain constants and the two conversion modes (`C`/`POSIX` and UTF-8).

/// `MB_CUR_MAX` of the `C`/`POSIX` locale (H-3): one byte per code unit.
pub const MB_CUR_MAX_BYTE: usize = 1;
/// `MB_CUR_MAX` of every UTF-8 locale (H-3).
pub const MB_CUR_MAX_UTF8: usize = 4;
/// `WEOF` as the unsigned 32-bit wide-value domain uses it.
pub const WEOF: u32 = u32::MAX;
/// The C `EOF` value.
pub const EOF: i32 = -1;

/// Conversion mode selected by the calling thread's locale.
///
/// `Byte` is the `C`/`POSIX` single-byte mode: bytes `0x80..=0xFF` pass through as code units.
/// `Utf8` validates and decodes UTF-8. Wide classification is locale independent (02 3.2).
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum MbMode {
    /// `C`/`POSIX`: byte-transparent, `MB_CUR_MAX == 1`.
    Byte,
    /// Every UTF-8 locale: `MB_CUR_MAX == 4`.
    Utf8,
}

impl MbMode {
    /// `MB_CUR_MAX` of this mode.
    #[must_use]
    pub const fn mb_cur_max(self) -> usize {
        match self {
            Self::Byte => MB_CUR_MAX_BYTE,
            Self::Utf8 => MB_CUR_MAX_UTF8,
        }
    }
}

#[cfg(test)]
#[path = "mode_ut.rs"]
mod tests;
