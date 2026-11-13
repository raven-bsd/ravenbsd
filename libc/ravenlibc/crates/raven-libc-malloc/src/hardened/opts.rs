// SPDX-License-Identifier: ISC
// Copyright (c) 2008, 2010, 2011, 2016, 2023 Otto Moerbeek <otto@drijf.net>
// Copyright (c) 2012 Matthew Dempsky <matthew@openbsd.org>
// Copyright (c) 2008 Damien Miller <djm@openbsd.org>
// Copyright (c) 2000 Poul-Henning Kamp <phk@FreeBSD.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: derived from openbsd:lib/libc/stdlib/malloc.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: malloc.c,v 1.302 2026/11/04 12:37:25 otto Exp $)
// The full ISC permission text and the Beerware remark are in hardened/mod.rs.

//! Option parsing for the hardened backend.
//!
//! Three sources in OpenBSD; this port keeps the build-time string and the environment (ignored
//! for a secure-exec process). The sysctl source is deliberately absent.

use crate::compat::options::Options;
use crate::errors::{Message, warning};
use crate::provider::Platform;

/// Parse the build string and, when allowed, `MALLOC_OPTIONS`.
///
/// Unknown letters produce the donor's one-line warning. The environment is ignored when
/// `Platform::is_secure_exec()` says so (T-12).
pub fn parse<X: Platform>(platform: &X, build_options: &[u8]) -> Options {
    let mut options = Options::default();
    report_unknown(platform, options.parse(build_options));
    if !platform.is_secure_exec()
        && let Some(environment) = platform.env_malloc_options()
    {
        let unknown = options.parse(environment);
        report_unknown(platform, unknown);
    }
    options
}

fn report_unknown<X: Platform>(platform: &X, letter: Option<u8>) {
    let Some(letter) = letter else {
        return;
    };
    let mut message = Message::new();
    message.push_str("unknown char in MALLOC_OPTIONS: ");
    message.push(letter);
    warning(platform, b"malloc", message.as_bytes());
}

/// Clamp a decoded pool count to the donor range.
#[must_use]
pub fn pool_count(options: &Options) -> usize {
    options
        .malloc_mutexes
        .clamp(2, crate::hardened::consts::MAX_POOLS)
}

/// Clamp the cache count to the donor range.
#[must_use]
pub fn cache_count(options: &Options) -> usize {
    options.def_maxcache.min(crate::hardened::consts::MAX_CACHE)
}

#[cfg(test)]
#[path = "opts_ut.rs"]
mod tests;
