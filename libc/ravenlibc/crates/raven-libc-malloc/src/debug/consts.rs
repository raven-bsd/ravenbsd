// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Debug-mode patterns, chosen here and deliberately unlike OpenBSD's `0xdb`/`0xdf` or the
//! well-known `deadbeef`/`feedface` families.

/// Redzone word (ASCII `RAVENZON`).
pub const REDZONE_WORD: u64 = 0x5241_5645_4e5a_4f4e;
/// Byte written to freshly allocated payloads.
pub const ALLOC_FILL: u8 = 0xa5;
/// Byte written to freed payloads.
pub const FREE_FILL: u8 = 0x5a;
/// Redzone bytes before and after a buffer.
pub const REDZONE: usize = 16;
/// Default quarantine length.
pub const DEFAULT_AUDIT: usize = 64;
/// Maximum quarantine length.
pub const MAX_AUDIT: usize = 4096;
/// Maximum live records in this first port.
pub const MAX_RECORDS: usize = 256;
/// Maximum stack frames recorded per allocation.
pub const MAX_STACK: usize = 8;
