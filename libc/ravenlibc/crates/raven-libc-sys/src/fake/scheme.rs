// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A minimal scheme world for the fake kernel.
//!
//! Mirrors `kernel/src/syscall/ipc.rs:40-121` (`SCHEME_REGISTER`, `OPEN`, `READ`, `WRITE`,
//! `CLOSE`, `DUP`, `FSTAT`, `CALL`): paths split at the first colon, an unregistered scheme is
//! `ENOENT`, and the server-owned file id is the handle payload.

use raven_abi::errno::Errno;

use host::collections::HashMap;
use host::sync::{Arc, Mutex};

/// A scheme server installed in the fake kernel.
pub trait SchemeHandler: Send + Sync {
    /// Open `suffix` (the bytes after `name:`) and return an opaque file id.
    fn open(&self, suffix: &[u8], flags: u32) -> Result<u32, Errno>;
    /// Read into `buffer`, returning the byte count.
    fn read(&self, file: u32, buffer: &mut [u8]) -> Result<usize, Errno>;
    /// Write from `data`, returning the byte count.
    fn write(&self, file: u32, data: &[u8]) -> Result<usize, Errno>;
    /// Return the 64-byte `FSTAT` record.
    fn fstat(&self, file: u32) -> Result<[u8; 64], Errno>;
    /// Typed RPC escape hatch.
    fn call(&self, file: u32, request: &[u8], response: &mut [u8]) -> Result<usize, Errno>;
}

/// The world of registered schemes.
#[derive(Clone, Default)]
pub struct SchemeWorld {
    servers: HashMap<host::vec::Vec<u8>, Arc<dyn SchemeHandler>>,
}

impl core::fmt::Debug for SchemeWorld {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("SchemeWorld")
            .field("servers", &self.servers.len())
            .finish()
    }
}

impl SchemeWorld {
    /// Register `name` (without the colon), replacing an older server.
    pub fn register(&mut self, name: &[u8], handler: Arc<dyn SchemeHandler>) {
        self.servers.insert(name.to_vec(), handler);
    }

    /// Find a server by name.
    pub fn lookup(&self, name: &[u8]) -> Option<Arc<dyn SchemeHandler>> {
        self.servers.get(name).map(Arc::clone)
    }

    /// Split a scheme path at the first colon.
    pub fn split(path: &[u8]) -> Result<(&[u8], &[u8]), Errno> {
        let index = path
            .iter()
            .position(|&byte| byte == b':')
            .ok_or(Errno::EINVAL)?;
        let (name, rest) = path.split_at(index);
        let rest = rest.get(1..).unwrap_or(&[]);
        if name.is_empty() || name.len() > 64 {
            return Err(Errno::EINVAL);
        }
        Ok((name, rest))
    }
}

struct MemFile {
    id: u32,
    path: host::vec::Vec<u8>,
    data: host::vec::Vec<u8>,
    cursor: usize,
}

/// A small in-memory file server used by tests and C55's process-lifetime test.
pub struct MemScheme {
    files: Mutex<host::vec::Vec<MemFile>>,
    next_id: Mutex<u32>,
}

impl Default for MemScheme {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl MemScheme {
    /// Build a scheme from `(path, contents)` pairs.
    pub fn new(files: &[(&[u8], &[u8])]) -> Self {
        let mut stored = host::vec::Vec::new();
        for (index, (path, data)) in files.iter().enumerate() {
            stored.push(MemFile {
                id: index as u32 + 1,
                path: path.to_vec(),
                data: data.to_vec(),
                cursor: 0,
            });
        }
        Self {
            files: Mutex::new(stored),
            next_id: Mutex::new(files.len() as u32 + 1),
        }
    }

    fn with_file<R>(
        &self,
        file: u32,
        f: impl FnOnce(&mut MemFile) -> Result<R, Errno>,
    ) -> Result<R, Errno> {
        let mut files = self.files.lock().unwrap_or_else(|error| error.into_inner());
        let entry = files
            .iter_mut()
            .find(|entry| entry.id == file)
            .ok_or(Errno::EBADF)?;
        f(entry)
    }
}

impl SchemeHandler for MemScheme {
    fn open(&self, suffix: &[u8], _flags: u32) -> Result<u32, Errno> {
        let mut files = self.files.lock().unwrap_or_else(|error| error.into_inner());
        let (path, data) = files
            .iter()
            .find(|entry| entry.path == suffix)
            .map(|entry| (entry.path.clone(), entry.data.clone()))
            .ok_or(Errno::ENOENT)?;
        let mut next = self
            .next_id
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let id = *next;
        *next = next.wrapping_add(1);
        files.push(MemFile {
            id,
            path,
            data,
            cursor: 0,
        });
        Ok(id)
    }

    fn read(&self, file: u32, buffer: &mut [u8]) -> Result<usize, Errno> {
        self.with_file(file, |entry| {
            let remaining = entry.data.len().saturating_sub(entry.cursor);
            let count = remaining.min(buffer.len());
            let source = entry
                .data
                .get(entry.cursor..entry.cursor + count)
                .ok_or(Errno::EIO)?;
            let destination = buffer.get_mut(..count).ok_or(Errno::EINVAL)?;
            destination.copy_from_slice(source);
            entry.cursor += count;
            Ok(count)
        })
    }

    fn write(&self, file: u32, data: &[u8]) -> Result<usize, Errno> {
        self.with_file(file, |entry| {
            let end = entry.cursor.checked_add(data.len()).ok_or(Errno::EINVAL)?;
            if end > entry.data.len() {
                entry.data.resize(end, 0);
            }
            let destination = entry.data.get_mut(entry.cursor..end).ok_or(Errno::EIO)?;
            destination.copy_from_slice(data);
            entry.cursor = end;
            Ok(data.len())
        })
    }

    fn fstat(&self, file: u32) -> Result<[u8; 64], Errno> {
        self.with_file(file, |entry| {
            let mut record = [0u8; 64];
            let size = (entry.data.len() as u64).to_le_bytes();
            record[8..16].copy_from_slice(&size);
            let blocks = ((entry.data.len() as u64).div_ceil(512)).to_le_bytes();
            record[16..24].copy_from_slice(&blocks);
            let mode = 0o100_644u32.to_le_bytes();
            record[48..52].copy_from_slice(&mode);
            let nlink = 1u32.to_le_bytes();
            record[52..56].copy_from_slice(&nlink);
            Ok(record)
        })
    }

    fn call(&self, file: u32, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
        self.with_file(file, |entry| {
            if request == b"size" {
                let text = host::format!("{}", entry.data.len());
                let bytes = text.as_bytes();
                let count = bytes.len().min(response.len());
                let destination = response.get_mut(..count).ok_or(Errno::EINVAL)?;
                destination.copy_from_slice(bytes.get(..count).ok_or(Errno::EINVAL)?);
                return Ok(count);
            }
            Err(Errno::EINVAL)
        })
    }
}
