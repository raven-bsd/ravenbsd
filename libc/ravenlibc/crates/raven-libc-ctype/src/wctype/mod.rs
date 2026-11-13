// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `<wctype.h>`: wide classification, names and case mapping (doc 02 3.2).
//!
//! The data-dependent classes and the simple case mappings come from the generated Unicode
//! 15.1.0 tables; the classes with an explicit definition in musl stay hard-coded (5.B row 16).
//! Wide classification is locale independent (O-2).

mod iswalnum;
mod iswalpha;
mod iswblank;
mod iswcntrl;
mod iswctype;
mod iswdigit;
mod iswgraph;
mod iswlower;
mod iswprint;
mod iswpunct;
mod iswspace;
mod iswupper;
mod iswxdigit;
mod towctrans;
mod wctrans;

pub use iswalnum::iswalnum;
pub use iswalpha::iswalpha;
pub use iswblank::iswblank;
pub use iswcntrl::iswcntrl;
pub use iswctype::{iswctype, wctype};
pub use iswdigit::iswdigit;
pub use iswgraph::iswgraph;
pub use iswlower::iswlower;
pub use iswprint::iswprint;
pub use iswpunct::iswpunct;
pub use iswspace::iswspace;
pub use iswupper::iswupper;
pub use iswxdigit::iswxdigit;
pub use towctrans::{towlower, towupper};
pub use wctrans::{towctrans, wctrans};
