// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel object allocator core: size classes, object caches and a `GlobalAlloc` shim.
//!
//! `raven-kmem` sits on top of the `raven-vmem` arena and adds the allocation machinery the
//! kernel heap needs before integration (A4): the general-purpose size classes, per-class free
//! lists over slabs, named object caches, alignment routing, and a [`GlobalAlloc`] shim.
//!
//! The crate is pure `no_std` + `alloc` and has no global or static state: the caller builds a
//! [`Vmem`] arena and hands it to [`Kmem::new`], which owns it. Everything is address-based and
//! safe (the safe modules carry `#![forbid(unsafe_code)]`); the only `unsafe` in the crate is the
//! [`KmemAlloc`] [`GlobalAlloc`] shim, whose raw-pointer contract is exactly the one the trait
//! defines.
//!
//! ```
//! use core::alloc::{GlobalAlloc, Layout};
//! use raven_kmem::{Kmem, KmemAlloc};
//! use raven_vmem::Vmem;
//!
//! let arena = Vmem::new(0x1000_0000, 0x10_0000).expect("arena");
//! let shim = KmemAlloc::new(Kmem::new(arena).expect("kmem"));
//! let layout = Layout::from_size_align(64, 8).expect("layout");
//! unsafe {
//!     let ptr = shim.alloc(layout);
//!     assert!(!ptr.is_null());
//!     shim.dealloc(ptr, layout);
//! }
//! ```

#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod cache;
#[cfg(feature = "debug")]
mod debug;
mod error;
mod global;
mod kmem;
mod magazine;
mod size;

pub use cache::CacheStats;
#[cfg(feature = "debug")]
pub use debug::{AUDIT_CAPACITY, AuditOp, AuditRecord, DebugAlloc, DebugReport, REDZONE_BYTES};
pub use error::KmemError;
pub use global::{KmemAlloc, KmemAllocGeneric};
pub use kmem::{CacheId, DEFAULT_MAX_SLABS, Kmem, KmemGeneric, KmemStats};
pub use magazine::{Cpu, MagCache, MagCacheStats, ROUNDS_MAX, ROUNDS_MIN, magazine_rounds};
pub use size::{LARGE_THRESHOLD, SIZE_CLASSES, class_align, class_for, classify};
