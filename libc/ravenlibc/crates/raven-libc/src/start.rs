// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Process start: C61 fills this file (`donor-map/10` 3.12.2).
//!
//! The skeleton is intentionally empty: `__raven_libc_start`, the C8 stack parse and the auxv
//! table belong to C61. C59's variadic module and C60's exports compile without it.
