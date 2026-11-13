// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `<ctype.h>`: ASCII classification and case mapping by arithmetic (A-ctype-1).
//!
//! Every function is total over `i32`: negative arguments, `EOF` and byte values `>= 0x80` are
//! never classified and never index a table. Bytes `>= 0x80` are not classified in either mode.

mod isalnum;
mod isalpha;
mod isascii;
mod isblank;
mod iscntrl;
mod isdigit;
mod isgraph;
mod islower;
mod isprint;
mod ispunct;
mod isspace;
mod isupper;
mod isxdigit;
mod toascii;
mod tolower;
mod toupper;

pub use isalnum::isalnum;
pub use isalpha::isalpha;
pub use isascii::isascii;
pub use isblank::isblank;
pub use iscntrl::iscntrl;
pub use isdigit::isdigit;
pub use isgraph::isgraph;
pub use islower::islower;
pub use isprint::isprint;
pub use ispunct::ispunct;
pub use isspace::isspace;
pub use isupper::isupper;
pub use isxdigit::isxdigit;
pub use toascii::toascii;
pub use tolower::tolower;
pub use toupper::toupper;
