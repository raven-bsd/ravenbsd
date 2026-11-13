// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The four seams of the allocator: pages, locks, entropy and the platform.
//!
//! Every method takes `&self`; the heap is shared by threads and the production providers use
//! interior mutability. This is the `&self` deviation recorded as `RD-new` (doc 08 4.6).

use crate::compat::{Mallinfo, Mallinfo2};

/// A source of page-aligned, zero-filled memory.
pub trait PageProvider {
    /// The platform page size; must be 4096.
    fn page_size(&self) -> usize;
    /// Map `len` zero-filled bytes with at least `align` alignment. The platform trims; the
    /// allocator never trims.
    fn map_pages(&self, len: usize, align: usize, prot: u8) -> Option<usize>;
    /// Map exactly `hint..hint + len`; fail if that range is busy (NOREPLACE).
    fn map_exact(&self, hint: usize, len: usize) -> Option<usize>;
    /// Release a range previously returned by this provider.
    fn unmap(&self, addr: usize, len: usize) -> bool;
    /// Change the protection of a mapped range.
    fn protect(&self, addr: usize, len: usize, prot: u8) -> bool;
    /// `mimmutable`: a no-op by default (A-malloc-3).
    fn seal(&self, _addr: usize, _len: usize) -> bool {
        true
    }
}

/// A non-allocating lock registry indexed `0..32`.
pub trait Lock {
    /// Acquire lock `index` (0..32).
    fn lock(&self, index: usize);
    /// Release lock `index`.
    fn unlock(&self, index: usize);
    /// Re-initialise lock `index` after `fork`.
    fn reset(&self, index: usize);
}

/// A random-byte source; `false` means failure, never a weak fallback.
pub trait Entropy {
    /// Fill `buf` with random bytes; return `false` on failure.
    fn fill(&self, buf: &mut [u8]) -> bool;
}

/// The C library platform hooks the allocator needs.
pub trait Platform {
    /// The kernel thread id used for pool selection.
    fn thread_id(&self) -> usize;
    /// Store an `errno` value.
    fn set_errno(&self, e: i32);
    /// Read the current `errno`; used to preserve it across `posix_memalign`.
    ///
    /// This is an additive seam beyond doc 08 4.2 so the C library can honour T-9 without a
    /// second errno location.
    fn get_errno(&self) -> i32 {
        0
    }
    /// Write bytes to standard error without stdio or allocation.
    fn write_stderr(&self, b: &[u8]);
    /// Terminate the process.
    fn abort(&self) -> !;
    /// Whether the process image is privileged; the default ignores the environment.
    fn is_secure_exec(&self) -> bool {
        true
    }
    /// The program name used by `wrterror`.
    fn progname(&self) -> &[u8];
    /// The process id used by `wrterror`.
    fn process_id(&self) -> u32;
    /// `MALLOC_OPTIONS` when the platform exposes it.
    fn env_malloc_options(&self) -> Option<&[u8]> {
        None
    }
    /// `MALLOC_DEBUG` when the platform exposes it.
    fn env_malloc_debug(&self) -> Option<&[u8]> {
        None
    }
    /// A heap summary for `mallinfo`; backends that keep counters override it.
    fn mallinfo(&self) -> Mallinfo {
        Mallinfo::default()
    }
    /// A heap summary for `mallinfo2`.
    fn mallinfo2(&self) -> Mallinfo2 {
        Mallinfo2::default()
    }
}

#[cfg(test)]
#[path = "provider_ut.rs"]
mod tests;
