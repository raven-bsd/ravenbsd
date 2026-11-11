// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Portable UFS1/UFS2 read core, ported from NetBSD FFS.
//!
//! This crate mounts an image through the caller-provided [`BlockIo`] device and exposes a
//! read-only filesystem: inode attributes, path lookup, directory iteration, file reads and
//! symlink targets. It has no OS types, no global state, no threads and no `unsafe` code. The
//! library is `no_std` + `alloc`; the host is responsible for the block device and the policy.
//!
//! Only little-endian UFS1 and UFS2 images are accepted. Big-endian images, the UFS2 extended
//! attribute variant, old 4.2BSD UFS1 inodes and write operations are rejected with
//! [`UfsError::Unsupported`] or [`UfsError::ReadOnly`]. Every on-disk value is decoded
//! explicitly from a byte slice and bounds-checked; a hostile image must return an error, never
//! panic or read outside the device.
//!
//! The on-disk logic is ported from the NetBSD sources named in `PROVENANCE.toml` and in the
//! header of each module. The kernel glue (vnodes, buffer cache, kauth, UVM) is replaced by the
//! small caches and the [`BlockIo`] trait.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc as alloc_crate;

pub mod alloc;

#[cfg(test)]
extern crate std;

mod balloc;
mod blockio;
mod dinode;
mod dir;
mod dirwrite;
mod error;
mod ffs_bitmap;
mod ffs_vfsops;
mod files;
mod fs;
mod inode;
pub mod mkfs;
mod namespace;
mod rename;
mod types;
mod ufs;
mod ufs_bmap;
mod ufs_lookup;
mod ufs_readwrite;
pub(crate) mod wapbl;

pub use blockio::BlockIo;
pub use error::{IoError, UfsError};
pub use types::{Attr, DirEntry, FileType, Ino, MountOptions, SetAttr, StatFs, Timespec};
pub use ufs::Ufs;
