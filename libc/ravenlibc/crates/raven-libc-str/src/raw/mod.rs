// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The audited pointer shell (DR-39).
//!
//! Every function here is Rust ABI and unsafe; the C-ABI shell (C60) wraps it in `extern "C"`.
//! No function validates what C does not validate. Each entry point wraps its pointer work in one
//! `unsafe` block whose `// SAFETY:` comment states the C-level contract; the compiler-builtins
//! units (`mem/impls.rs`, `arch/*/mem.rs`) are one audited implementation each.
//!
//! The eight recursion-sensitive bodies (`memcpy memmove memset memcmp bcmp strlen strcpy
//! stpcpy`) use only the vendor loops: no `core::ptr::copy*`, no `write_bytes`, no slice
//! equality, no `CStr` (A-str-2, ticket 5.3).

mod common;

pub mod mem;

pub mod arch;

pub mod bsd_compat;
pub mod explicit_bzero;
pub mod gnu_basename;
pub mod memccpy;
pub mod memchr;
pub mod memmem;
pub mod mempcpy;
pub mod memset_s;
#[cfg(feature = "test-probes")]
pub mod probes;
pub mod rawmemchr;
pub mod stpcpy;
pub mod strcasecmp;
pub mod strchr;
pub mod strcmp;
pub mod strcpy;
pub mod strdup;
pub mod strerror;
pub mod strlcpy;
pub mod strlen;
pub mod strmode;
pub mod strnlen;
pub mod strsep;
pub mod strspn;
pub mod strstr;
pub mod strtok;
pub mod strverscmp;
pub mod swab;

pub use bsd_compat::{bcopy, bzero, index, rindex};
pub use explicit_bzero::{explicit_bzero, memset_explicit};
pub use gnu_basename::gnu_basename;
pub use mem::{bcmp, memcmp, memcpy, memmove, memset};
pub use memccpy::memccpy;
pub use memchr::{memchr, memrchr};
pub use memmem::memmem;
pub use mempcpy::mempcpy;
pub use memset_s::memset_s;
pub use rawmemchr::rawmemchr;
pub use stpcpy::{stpcpy, stpncpy, strncpy};
pub use strcasecmp::{strcasecmp, strncasecmp};
pub use strchr::{strchr, strchrnul, strrchr};
pub use strcmp::{strcmp, strncmp};
pub use strcpy::{strcat, strcpy, strncat};
pub use strdup::{strdup_with, strndup_with};
pub use strerror::{gnu_strerror_r, strerror_r, strsignal};
pub use strlcpy::{strlcat, strlcpy};
pub use strlen::strlen;
pub use strmode::strmode;
pub use strnlen::strnlen;
pub use strsep::strsep;
pub use strspn::{strcspn, strpbrk, strspn};
pub use strstr::{strcasestr, strnstr, strstr};
pub use strtok::{strtok, strtok_r};
pub use strverscmp::strverscmp;
pub use swab::swab;
