// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Memory glue (T5).

pub mod mmap;
pub mod shm;

pub use mmap::{
    brk, getpagesize, madvise, mlock, mlockall, mmap, mprotect, mremap, msync, munlock, munlockall,
    munmap,
};
pub use shm::{path as shm_path, shm_open, shm_unlink};

#[cfg(test)]
#[path = "mem_ut.rs"]
mod tests;
