// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<wctype.h>` exports over the UTF-8 Unicode tables. The `_l` forms accept a locale object
//! and use the same C/UTF-8 classification until the locale-aware tables land.

pub(crate) mod iswalnum_l;
pub(crate) mod iswctype_l;
pub(crate) mod iswdigit_l;
pub(crate) mod iswpunct_l;
pub(crate) mod wctrans_l;
