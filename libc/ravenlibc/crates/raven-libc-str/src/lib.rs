// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The ravenlibc `<string.h>`, `<strings.h>` and `<stdbit.h>` family.
//!
//! Slice-level engines (`mem`, `cstr`, `secure`, `bits`, `stdbit`, `strmode`, `errmsg`,
//! `gnu_basename`, `itoa`) plus exactly one audited pointer module, [`raw`], that the C-ABI shell
//! (C60) turns into `extern "C"` symbols. Nothing here calls `errno`, allocates, or keeps global
//! state: message buffers, the `strtok` cursor and the `malloc` function of `strdup` are caller
//! parameters (RD-13, RD-14).
//!
//! The crate is `no_std`, has no `alloc` (R-8), must never use `core::fmt` (R-5), and carries
//! `#![no_builtins]` so LLVM cannot rewrite a copy loop into a call to the very `memcpy` being
//! defined (A-str-2). Every `unsafe` block lives in [`raw`] and carries a `// SAFETY:` comment
//! (DR-39).

#![cfg_attr(not(test), no_std)]
#![no_builtins]
#![deny(unsafe_code)]
#![allow(unexpected_cfgs)] // `raven_strict_reads` is a documented external cfg (A-str-1)
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

mod ascii;
pub mod bits;
pub mod cstr;
pub mod errmsg;
pub mod gnu_basename;
pub mod itoa;
pub mod mem;
#[allow(unsafe_code)]
pub mod raw;
pub mod secure;
pub mod stdbit;
pub mod strmode;
