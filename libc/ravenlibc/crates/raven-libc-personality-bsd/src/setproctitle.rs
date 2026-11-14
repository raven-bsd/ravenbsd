// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `setproctitle`/`setproctitle_fast` (`B04`, `10` 3.11).
//!
//! The argv/environ area is **never** overwritten: the title goes to `procd` as a `SETTITLE`
//! message and `ps` reads it there. The title is `"<progname>: <text>"`; a rendered format that
//! starts with `-` suppresses the prefix; a `NULL` format restores the original; the stored
//! title is truncated to 255 bytes (PROPOSED). `setproctitle_fast` skips the call when the
//! rendered title equals the cached one. The variadic formatting belongs to the shell.

use alloc::vec::Vec;

use crate::seams::{Errno, ProcCtl};

/// The stored title limit (PROPOSED; `10` 3.11).
pub const MAX_TITLE: usize = 255;

/// The last title the process sent, for the fast path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cache {
    last: Option<Vec<u8>>,
}

impl Cache {
    /// A fresh cache.
    #[must_use]
    pub const fn new() -> Self {
        Self { last: None }
    }

    /// The cached title.
    #[must_use]
    pub fn last(&self) -> Option<&[u8]> {
        self.last.as_deref()
    }
}

/// Render the stored title; `None` means "restore the original".
#[must_use]
pub fn format_title(progname: &[u8], rendered: Option<&[u8]>) -> Option<Vec<u8>> {
    let rendered = rendered?;
    let mut title = if let Some(rest) = rendered.strip_prefix(b"-") {
        rest.to_vec()
    } else if progname.is_empty() {
        rendered.to_vec()
    } else {
        let mut value = Vec::new();
        value.extend_from_slice(progname);
        value.extend_from_slice(b": ");
        value.extend_from_slice(rendered);
        value
    };
    if title.len() > MAX_TITLE {
        title.truncate(MAX_TITLE);
    }
    Some(title)
}

/// `setproctitle`; `fast` selects the cached fast path of `setproctitle_fast`.
pub fn setproctitle<P: ProcCtl>(
    process: &mut P,
    cache: &mut Cache,
    progname: &[u8],
    rendered: Option<&[u8]>,
    fast: bool,
) -> Result<(), Errno> {
    let title = format_title(progname, rendered);
    if fast && cache.last == title {
        return Ok(());
    }
    match &title {
        Some(value) => process.set_title(Some(value))?,
        None => process.set_title(None)?,
    }
    cache.last = title;
    Ok(())
}

#[cfg(test)]
#[path = "setproctitle_ut.rs"]
mod tests;
