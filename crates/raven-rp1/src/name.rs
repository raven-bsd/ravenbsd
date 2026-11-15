// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fixed-capacity names copied from the device tree.
//!
//! The MFD model is `no_std` and allocation-free, so a pin function or owner name lives in a
//! small byte array. Names longer than [`NAME_MAX`] are rejected, not truncated: a truncated
//! function name would silently stop matching a device-tree rule.

use core::fmt;

/// Longest name the model stores.
pub const NAME_MAX: usize = 16;

/// Why a name does not fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    /// The name is empty.
    Empty,
    /// The name is longer than [`NAME_MAX`].
    TooLong(usize),
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty name"),
            Self::TooLong(length) => write!(f, "name of {length} bytes exceeds {NAME_MAX}"),
        }
    }
}

impl core::error::Error for NameError {}

/// A short name with inline storage.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Name {
    bytes: [u8; NAME_MAX],
    len: u8,
}

impl Name {
    /// Copy `text` into the inline storage.
    ///
    /// # Errors
    ///
    /// Returns [`NameError`] for an empty or oversized name.
    pub fn new(text: &str) -> Result<Self, NameError> {
        let source = text.as_bytes();
        if source.is_empty() {
            return Err(NameError::Empty);
        }
        if source.len() > NAME_MAX {
            return Err(NameError::TooLong(source.len()));
        }
        let mut bytes = [0u8; NAME_MAX];
        for (slot, byte) in bytes.iter_mut().zip(source) {
            *slot = *byte;
        }
        Ok(Self {
            bytes,
            len: source.len() as u8,
        })
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        let bytes = self.bytes.get(..usize::from(self.len)).unwrap_or(&[]);
        core::str::from_utf8(bytes).unwrap_or("")
    }

    /// Number of bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// True when the name is empty (only a default value can be).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[path = "name_ut.rs"]
mod tests;
