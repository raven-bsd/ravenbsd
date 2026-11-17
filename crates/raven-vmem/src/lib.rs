// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Virtual-memory arena allocator.
//!
//! `raven-vmem` is the pure core under the kernel object allocator (`raven-kmem`). It manages a
//! caller-supplied set of integer ranges — virtual address space today, IDs, ports and interrupt
//! vectors later — as boundary-tagged spans with neighbour coalescing.
//!
//! The crate is `no_std` + `alloc`, contains no unsafe code and holds no global or static state.
//! Constructors allocate the fixed tag slab; [`Vmem::alloc`], [`Vmem::alloc_at`], [`Vmem::free`]
//! and [`Vmem::import`] never allocate, so the arena can itself back an allocator.
//!
//! ```
//! use raven_vmem::Vmem;
//!
//! let mut arena = Vmem::new(0x1_0000, 0x4000).expect("valid arena");
//! let base = arena.alloc(0x1000, 0x1000).expect("free span");
//! assert_eq!(base, 0x1_0000);
//! assert!(arena.contains(base));
//! arena.free(base, 0x1000).expect("allocated segment");
//! assert_eq!(arena.span_count(), 1);
//! ```

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod arena;
mod error;
mod segment;

pub use arena::{DEFAULT_TAG_CAPACITY, Vmem, VmemStats};
pub use error::{Policy, VmemError};
pub use segment::{Segment, SegmentState};
