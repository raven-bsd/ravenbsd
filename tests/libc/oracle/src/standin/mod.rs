// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Plain-Rust stand-ins for the worked functions.
//!
//! These are the subjects until the real `raven-libc-str` crate lands (C40 replaces them as the
//! second subject but keeps them as the mutation base). They are written fresh from the C
//! semantics, not transpiled: no provenance entry, no `RavenBSD: transpiled` marker.

pub mod memchr;
pub mod strlcpy;
pub mod strnlen;
