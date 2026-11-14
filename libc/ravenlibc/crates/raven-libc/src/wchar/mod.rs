// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<wchar.h>` and `<uchar.h>` exports (`02-api-surface` I24 X27, the conversion core).

pub(crate) mod btowc;
pub(crate) mod mblen;
pub(crate) mod mbrlen;
pub(crate) mod mbrtowc;
pub(crate) mod mbtowc;
pub(crate) mod wcrtomb;
pub(crate) mod wcs;
pub(crate) mod wcwidth;
