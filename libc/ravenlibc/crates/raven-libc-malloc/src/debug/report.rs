// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Leak-report formatting into a caller-supplied sink.

use crate::errors::Message;

/// One group of live buffers with the same size class and stack head.
#[derive(Clone, Copy, Default)]
pub struct Group {
    /// Size class (the requested size).
    pub size: usize,
    /// First stack word, or zero.
    pub stack: usize,
    /// Number of buffers.
    pub count: usize,
    /// Total bytes.
    pub bytes: usize,
}

/// Emit one report line.
pub fn emit<F: FnMut(&[u8]) + ?Sized>(out: &mut F, group: &Group) {
    let mut line = Message::new();
    line.push_dec(group.count);
    line.push_str(" x ");
    line.push_dec(group.size);
    line.push_str(" bytes from ");
    if group.stack == 0 {
        line.push_str("n/a");
    } else {
        line.push_ptr(group.stack);
    }
    line.push(b'\n');
    out(line.as_bytes());
}
