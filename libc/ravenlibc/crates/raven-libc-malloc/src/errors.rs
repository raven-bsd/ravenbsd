// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Hand-formatted diagnostics for `wrterror`.
//!
//! The allocator must not call stdio, `core::fmt` or `malloc` from a failure path (T-5). This is a
//! bounded byte builder plus decimal/hex/pointer formatting, about forty lines of arithmetic.

use crate::provider::Platform;

/// Maximum diagnostic line held on the stack.
pub const MAX_MESSAGE: usize = 256;

/// A fixed-size message builder.
pub struct Message {
    buffer: [u8; MAX_MESSAGE],
    len: usize,
}

impl Message {
    /// An empty message.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buffer: [0; MAX_MESSAGE],
            len: 0,
        }
    }

    /// Append one byte.
    pub fn push(&mut self, byte: u8) {
        if let Some(slot) = self.buffer.get_mut(self.len) {
            *slot = byte;
            self.len = self.len.wrapping_add(1);
        }
    }

    /// Append bytes.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        let mut index = 0usize;
        while let Some(&byte) = bytes.get(index) {
            self.push(byte);
            index = index.wrapping_add(1);
        }
    }

    /// Append a string literal.
    pub fn push_str(&mut self, text: &str) {
        self.push_bytes(text.as_bytes());
    }

    /// Append an unsigned decimal number.
    pub fn push_dec(&mut self, mut value: usize) {
        let mut digits = [0u8; 20];
        let mut count = 0usize;
        if value == 0 {
            self.push(b'0');
            return;
        }
        while value != 0 {
            if let Some(slot) = digits.get_mut(count) {
                *slot = b'0' + (value % 10) as u8;
            }
            value /= 10;
            count = count.wrapping_add(1);
        }
        while count != 0 {
            count = count.wrapping_sub(1);
            if let Some(&digit) = digits.get(count) {
                self.push(digit);
            }
        }
    }

    /// Append a lowercase hexadecimal number.
    pub fn push_hex(&mut self, mut value: usize) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut digits = [0u8; 16];
        let mut count = 0usize;
        if value == 0 {
            self.push(b'0');
            return;
        }
        while value != 0 {
            if let (Some(slot), Some(&digit)) = (digits.get_mut(count), HEX.get(value & 0xf)) {
                *slot = digit;
            }
            value >>= 4;
            count = count.wrapping_add(1);
        }
        while count != 0 {
            count = count.wrapping_sub(1);
            if let Some(&digit) = digits.get(count) {
                self.push(digit);
            }
        }
    }

    /// Append a pointer in the `0x` form used by the donor messages.
    pub fn push_ptr(&mut self, value: usize) {
        self.push_str("0x");
        self.push_hex(value);
    }

    /// The bytes written so far.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.buffer.get(..self.len).unwrap_or(&[])
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::new()
    }
}

/// Print `"<progname>(<pid>) in <function>(): <message>\n"` and abort.
pub fn wrterror<X: Platform>(platform: &X, function: &[u8], message: &Message) -> ! {
    let mut line = Message::new();
    line.push_bytes(platform.progname());
    line.push(b'(');
    line.push_dec(platform.process_id() as usize);
    line.push_str(") in ");
    line.push_bytes(function);
    line.push_str("(): ");
    line.push_bytes(message.as_bytes());
    line.push(b'\n');
    platform.write_stderr(line.as_bytes());
    platform.abort()
}

/// Print a warning and return.
pub fn warning<X: Platform>(platform: &X, function: &[u8], message: &[u8]) {
    let mut line = Message::new();
    line.push_bytes(platform.progname());
    line.push(b'(');
    line.push_dec(platform.process_id() as usize);
    line.push_str(") in ");
    line.push_bytes(function);
    line.push_str("(): ");
    line.push_bytes(message);
    line.push(b'\n');
    platform.write_stderr(line.as_bytes());
}

#[cfg(test)]
#[path = "errors_ut.rs"]
mod tests;
