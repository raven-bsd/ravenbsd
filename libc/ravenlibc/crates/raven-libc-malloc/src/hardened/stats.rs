// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `MALLOC_STATS` is not translated.
//!
//! OpenBSD's statistics, `malloc_dump`, leak report and `SET_CALLER` machinery (malloc.c lines
//! 2357-2770) need `utrace`, `dlinfo` and `__builtin_return_address`, none of which stable Rust
//! or Raven offers. `mallinfo`/`malloc_stats` use the always-present counters instead
//! (O-8.5, A-malloc-16).
