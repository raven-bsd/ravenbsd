// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Boot information for boot paths that start from a flattened device tree.
//!
//! The crate has two independent halves:
//!
//! * [`imp`] turns an FDT into a [`raven_bootinfo::BootInfo`] and a memory map, and parses the
//!   platform description (GIC, PL011, timer PPI, board model).
//! * [`paging`] builds the minimal aarch64 translation tables a Pi 4 entry stub needs before the
//!   kernel installs its own tables.
//!
//! Both are pure `no_std` code with no allocation and no panics; the Pi 4 entry stub compiles the
//! same source into the kernel with `#[path]`, while `cargo test -p raven-bootinfo-fdt` exercises
//! it on the host with synthetic device trees.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate alloc;

/// The FDT view used by the BootInfo builder. The kernel substitutes its own re-export of the same
/// `raven-fdt-lite` types when it compiles [`imp`] with `#[path]`.
pub(crate) mod fdt {
    pub use raven_fdt_lite::*;
}

mod imp;
pub mod paging;

pub use imp::*;
