// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Memory caps.
//!
//! Every cap is an error value when exceeded, never a truncation. The numbers are the ones
//! fixed by the area document (13 4.1) and bound the work a hostile image can request.

/// Maximum program headers in an object.
pub const MAX_PHDRS: usize = 64;
/// Maximum `PT_LOAD` segments in one object.
pub const MAX_LOAD_SEGMENTS: usize = 16;
/// Maximum `DT_NEEDED` entries recorded for one object.
pub const MAX_NEEDED: usize = 256;
/// Maximum entries walked in one `_DYNAMIC` array.
pub const MAX_DYN_ENTRIES: usize = 512;
/// Maximum relocations applied for one object.
pub const MAX_RELOCS_PER_OBJECT: usize = 4_000_000;
/// Maximum symbol table entries (GNU-hash chain step limit).
pub const MAX_SYMBOLS: usize = 1_000_000;
/// Maximum version definitions plus requirements recorded for one object.
pub const MAX_VERSIONS: usize = 4096;
/// Maximum length of one search path list (`LD_LIBRARY_PATH`).
pub const MAX_SEARCH_PATH_LEN: usize = 65_536;
/// Maximum candidate paths for one library.
pub const MAX_SEARCH_CANDIDATES: usize = 1024;
/// Maximum length of one path (`PATH_MAX`).
pub const MAX_PATH_LEN: usize = 4096;
/// Maximum entries in one `DT_INIT_ARRAY`-shaped array.
pub const MAX_INIT_ARRAY: usize = 4096;
/// Maximum objects in one walk of a scope list.
pub const MAX_SCOPE_OBJECTS: usize = 4096;

#[cfg(test)]
#[path = "limits_ut.rs"]
mod tests;
