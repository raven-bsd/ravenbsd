// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The one 512-byte `dlerror` buffer.

use core::fmt;

/// A fixed error buffer.
pub struct ErrBuf {
    bytes: [u8; 512],
    len: usize,
    pending: bool,
}

impl ErrBuf {
    /// An empty buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bytes: [0; 512],
            len: 0,
            pending: false,
        }
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.len = 0;
        self.pending = false;
    }

    /// Format a message into the buffer.
    pub fn set(&mut self, args: fmt::Arguments<'_>) {
        let mut writer = Writer {
            bytes: &mut self.bytes,
            len: 0,
        };
        let _ = fmt::write(&mut writer, args);
        self.len = writer.len.min(self.bytes.len());
        self.pending = true;
    }

    /// The current message, without consuming it.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or(&[])
    }

    /// Take the message once; a second call returns `None`.
    pub fn take_once(&mut self) -> Option<&[u8]> {
        if self.pending {
            self.pending = false;
            Some(self.bytes.get(..self.len).unwrap_or(&[]))
        } else {
            None
        }
    }
}

impl Default for ErrBuf {
    fn default() -> Self {
        Self::new()
    }
}

struct Writer<'a> {
    bytes: &'a mut [u8; 512],
    len: usize,
}

impl fmt::Write for Writer<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let room = self.bytes.len().saturating_sub(self.len);
        let take = room.min(text.len());
        if let (Some(dst), Some(src)) = (
            self.bytes.get_mut(self.len..self.len.saturating_add(take)),
            text.as_bytes().get(..take),
        ) {
            dst.copy_from_slice(src);
        }
        self.len = self.len.saturating_add(take);
        Ok(())
    }
}
