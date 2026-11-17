// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The filesystem seam used by the uucp spool and queue models.

use alloc::string::String;
use alloc::vec::Vec;

/// Why a spool operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpoolError {
    /// The requested path does not exist.
    NotFound,
    /// The filesystem reported an I/O error.
    Io,
    /// A path component is not valid for the spool.
    InvalidPath,
}

/// The bounded filesystem operations the queue needs.
///
/// The target implementation uses `raven_rt::fs`; host tests use `std::fs` or an in-memory map.
/// Paths are absolute, `/`-separated strings.
pub trait Spool {
    /// Read a whole file.
    fn read(&mut self, path: &str) -> Result<Vec<u8>, SpoolError>;
    /// Write a whole file, creating or truncating it.
    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), SpoolError>;
    /// Remove a file.
    fn remove(&mut self, path: &str) -> Result<(), SpoolError>;
    /// List the immediate child names of a directory.
    fn list(&mut self, path: &str) -> Result<Vec<String>, SpoolError>;
    /// Create a directory and its missing parents; an existing directory is success.
    fn create_dir_all(&mut self, path: &str) -> Result<(), SpoolError>;
    /// Whether a path is a directory.
    fn is_dir(&mut self, path: &str) -> Result<bool, SpoolError>;
    /// Whether a path exists.
    fn exists(&mut self, path: &str) -> bool;
    /// Read the permission bits of a path.
    fn mode(&mut self, path: &str) -> Result<u32, SpoolError>;
    /// Set the permission bits of a path.
    fn set_mode(&mut self, path: &str, mode: u32) -> Result<(), SpoolError>;
}
