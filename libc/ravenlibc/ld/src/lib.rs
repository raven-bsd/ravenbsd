// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ld-elf.so.1`, the RavenBSD dynamic loader.
//!
//! The loader is host-testable: the mapping, allocation, linking and `dl*` logic is generic over
//! the [`mapper::Platform`] seam and the C52 `MemView`/`RelocApplier` traits. The raw target
//! implementation and the entry assembly live in [`raw`] and are compiled only for
//! `target_os = "none"`.

#![cfg_attr(all(not(test), target_os = "none"), no_std)]
#![deny(unsafe_code)]
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
// The loader's fixed-size tables are indexed with constants and bounds checked at the entry;
// the address arithmetic is the C `size_t` arithmetic of the donor. These two restriction lints
// would require a fallible form on every line and obscure the port.
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod arch;
pub mod dlfcn;
pub mod errbuf;
pub mod initfini;
pub mod ldalloc;
pub mod loadobj;
pub mod lock;
pub mod main_flow;
pub mod mapper;
pub mod rdebug;
pub mod relocate;
pub mod search;
pub mod self_object;
pub mod table;
pub mod tls;
pub mod unload;
pub mod xalloc;

#[cfg(target_os = "none")]
#[allow(unsafe_code)]
pub mod raw;

// The target allocator/panic handler live in `raw`; host tests and the host library use std.

pub use mapper::{FileId, LoaderFile, MapErrKind, MapSource, Mapper, Platform, Prot, map_object};
pub use table::RtldTable;
