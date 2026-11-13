// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Memory glue (T5).

pub mod mmap;

pub use mmap::{
    brk, getpagesize, madvise, mlock, mlockall, mmap, mprotect, mremap, msync, munlock, munlockall,
    munmap,
};

#[cfg(test)]
#[path = "mem_ut.rs"]
mod tests;
