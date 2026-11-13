// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The C-string engines: every function takes string bytes **without** the terminating NUL.
//!
//! The slice end acts as the NUL terminator (`byte_at`), so a caller can pass either the exact
//! byte string or a longer, NUL-terminated view. Embedded NULs end the string exactly as in C.

pub mod strcasecmp;
pub mod strcasestr;
pub mod strcmp;
pub mod strlcat;
pub mod strlcpy;
pub mod strncat;
pub mod strncpy;
pub mod strnstr;
pub mod strsep;
pub mod strspn;
pub mod strstr;
pub mod strtok;
pub mod strverscmp;

pub use strcasecmp::{strcasecmp, strncasecmp};
pub use strcasestr::strcasestr;
pub use strcmp::{strcmp, strncmp};
pub use strlcat::strlcat;
pub use strlcpy::strlcpy;
pub use strncat::strncat_len;
pub use strncpy::strncpy;
pub use strnstr::strnstr;
pub use strsep::strsep;
pub use strspn::{strcspn, strpbrk, strspn};
pub use strstr::strstr;
pub use strtok::strtok;
pub use strverscmp::strverscmp;

/// Read byte `index` of a C string; past the end the value is the terminating NUL.
#[inline]
pub(crate) fn byte_at(bytes: &[u8], index: usize) -> u8 {
    bytes.get(index).copied().unwrap_or(0)
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
