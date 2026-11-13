// SPDX-License-Identifier: ISC
// Copyright (c) 2008 Otto Moerbeek <otto@drijf.net>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: derived from openbsd:lib/libc/stdlib/reallocarray.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: reallocarray.c,v 1.3 2015/09/13 08:31:47 guenther Exp $)
// The full ISC permission text is in hardened/mod.rs.

//! OpenBSD `reallocarray` size check.

pub use crate::compat::checks::array_size;

#[cfg(test)]
#[path = "array_ut.rs"]
mod tests;
