// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Content and glyph provider seams.
//!
//! E3 supplies the network/metadata fetch behind [`Content`]; E1c owns only the render contract.
//! A page is either a borrowed 1-bit bitmap or borrowed text. Text is rendered through a [`Font`]
//! provider, so the glyph table (and its licensing) is an explicit E3 input rather than a hidden
//! asset in the service core.

use raven_eink::Glyph;

use crate::Error;

/// One fetched page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page<'a> {
    /// A 1-bit MSB-first bitmap; a set bit paints black.
    Bitmap {
        /// Bitmap width in pixels.
        width: usize,
        /// Bitmap height in pixels.
        height: usize,
        /// Row-major bitmap bytes.
        data: &'a [u8],
    },
    /// UTF-8/ASCII text rendered with a [`Font`].
    Text {
        /// Text to render; `\n` starts a new line.
        text: &'a str,
    },
}

/// Fetches the next page.
///
/// A provider may borrow its own buffer for the returned page. A failed fetch must leave the
/// previously displayed frame untouched; the service enforces that by keeping the last-good copy.
pub trait Content {
    /// Fetch the next page, or an error that the service retries with backoff.
    fn fetch(&mut self) -> Result<Page<'_>, Error>;
}

/// A glyph provider for text pages.
pub trait Font {
    /// Return the glyph for one character, or `None` for an unsupported character.
    fn glyph(&self, character: u8) -> Option<Glyph<'_>>;
}
