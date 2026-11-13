// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C-shaped allocator interface shared by both backends.
//!
//! The shell crate exports `malloc` and friends; it calls this trait only. Addresses are `usize`,
//! zero means `NULL`, and each backend sets `errno` through [`crate::provider::Platform`].

pub mod aliases;
pub mod array;
pub mod checks;
pub mod freezero;
pub mod info;
pub mod options;
pub mod reallocf;
pub mod recallocarray;
pub mod uniform;
pub mod usable;

#[cfg(test)]
pub(crate) mod testheap;

/// `struct mallinfo` (the glibc field layout, ten `int`s).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mallinfo {
    /// Bytes obtained from the system and still held.
    pub arena: i32,
    /// Free blocks.
    pub ordblks: i32,
    /// Small free blocks.
    pub smblks: i32,
    /// Page-sized allocations.
    pub hblks: i32,
    /// Bytes in page-sized allocations.
    pub hblkhd: i32,
    /// Maximum total allocated space (unused).
    pub usmblks: i32,
    /// Small free bytes.
    pub fsmblks: i32,
    /// Bytes in use.
    pub uordblks: i32,
    /// Free bytes.
    pub fordblks: i32,
    /// Keep cost (unused).
    pub keepcost: i32,
}

/// `struct mallinfo2` (the same fields with `size_t`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mallinfo2 {
    /// Bytes obtained from the system and still held.
    pub arena: usize,
    /// Free blocks.
    pub ordblks: usize,
    /// Small free blocks.
    pub smblks: usize,
    /// Page-sized allocations.
    pub hblks: usize,
    /// Bytes in page-sized allocations.
    pub hblkhd: usize,
    /// Maximum total allocated space (unused).
    pub usmblks: usize,
    /// Small free bytes.
    pub fsmblks: usize,
    /// Bytes in use.
    pub uordblks: usize,
    /// Free bytes.
    pub fordblks: usize,
    /// Keep cost (unused).
    pub keepcost: usize,
}

/// One allocator backend.
pub trait Heap {
    /// Allocate `size` bytes; zero means a unique non-NULL minimum block.
    fn malloc(&self, size: usize) -> usize;
    /// Free a pointer returned by this heap.
    fn free(&self, p: usize);
    /// Resize `p` to `size` bytes.
    fn realloc(&self, p: usize, size: usize) -> usize;
    /// Allocate `n * m` zeroed bytes with overflow checking.
    fn calloc(&self, n: usize, m: usize) -> usize;
    /// Allocate `size` bytes with `align` alignment.
    fn memalign(&self, align: usize, size: usize) -> usize;
    /// The writable capacity of `p`.
    fn usable_size(&self, p: usize) -> usize;
    /// Release cached pages; true when something was released.
    fn trim(&self) -> bool;
    /// Current heap statistics.
    fn info(&self) -> Mallinfo2;
    /// C23 `free_sized`.
    fn free_sized(&self, p: usize, size: usize);
    /// C23 `free_aligned_sized`.
    fn free_aligned_sized(&self, p: usize, align: usize, size: usize);
    /// Lock the heap before `fork`.
    fn prefork(&self);
    /// Unlock in the parent after `fork`.
    fn postfork_parent(&self);
    /// Re-initialise locks in the child after `fork`.
    fn postfork_child(&self);
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
