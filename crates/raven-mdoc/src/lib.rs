// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A small, first-party renderer for the mdoc subset used by the RavenBSD manual pages.
//!
//! The renderer is `no_std` + `alloc`. It emits semantic spans carrying a [`raven_theme::Role`]
//! so the caller can paint them with the selected palette; it never emits ANSI itself. Unknown
//! macros fall back to their argument text and never abort a page.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod document;

pub use document::{DEFAULT_WIDTH, Document, Error, Line, Span, render};
