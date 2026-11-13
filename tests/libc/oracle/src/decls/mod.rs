// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C oracle declarations.
//!
//! Each family has one module with the `#[link]` attribute on its extern block. With a family
//! feature off the module is not compiled; with it on, the test binary links
//! `liboracle_<family>.so` from `LIBRARY_PATH` and loads it through `LD_LIBRARY_PATH`.

#[cfg(feature = "oracle-string-basic")]
pub mod string_basic;

pub mod str_common;
#[cfg(feature = "oracle-str-errmsg")]
pub mod str_errmsg;
#[cfg(feature = "oracle-str-freebsd")]
pub mod str_freebsd;
#[cfg(feature = "oracle-str-mem")]
pub mod str_mem;
#[cfg(feature = "oracle-str-musl")]
pub mod str_musl;
#[cfg(feature = "oracle-str-openbsd")]
pub mod str_openbsd;
