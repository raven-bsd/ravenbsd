// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side UFS image checking and repair.

#![forbid(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use ufs_core::{BlockIo, IoError};

/// A file-backed block device.
pub struct FileDisk {
    file: File,
    size: u64,
}

impl FileDisk {
    /// Open an existing image for reading, or for reading and writing.
    pub fn open(path: &Path, writable: bool) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(writable)
            .open(path)
            .map_err(|error| format!("open {}: {error}", path.display()))?;
        let size = file
            .metadata()
            .map_err(|error| format!("stat {}: {error}", path.display()))?
            .len();
        Ok(Self { file, size })
    }
}

impl BlockIo for FileDisk {
    fn size_bytes(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| IoError)?;
        self.file.read_exact(buf).map_err(|_| IoError)
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| IoError)?;
        self.file.write_all(buf).map_err(|_| IoError)
    }

    fn flush(&mut self) -> Result<(), IoError> {
        self.file.sync_all().map_err(|_| IoError)
    }
}
