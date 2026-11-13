// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The one in-memory file system on which both the C oracle and the Rust subject run.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EBADF, EINVAL, ENOENT, ESPIPE};
use raven_libc_stdio::{Lock, StatLite, SysFile};

/// The descriptor kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A seekable regular file.
    Regular,
    /// A character device that is a terminal.
    Tty,
    /// An unseekable pipe.
    Pipe,
}

/// One open descriptor.
#[derive(Clone)]
pub struct Entry {
    /// The data.
    pub data: Vec<u8>,
    /// The cursor.
    pub pos: usize,
    /// The `O_*` status flags.
    pub fl: i32,
    /// The kind.
    pub kind: Kind,
    /// The path, when the descriptor came from `open`.
    pub path: Option<Vec<u8>>,
}

/// One trace record (write/seek/close/open only; reads are strategy dependent).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TraceOp {
    /// 0 open, 1 write, 2 seek, 3 close.
    pub op: u8,
    /// The descriptor.
    pub fd: i32,
    /// The requested length (write) or the offset (seek).
    pub len: i64,
    /// The result (fd, bytes written or the new offset).
    pub result: i64,
}

/// The shared world.
pub struct Store {
    /// Open descriptors.
    pub files: BTreeMap<i32, Entry>,
    /// Path contents.
    pub contents: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Next descriptor.
    pub next_fd: i32,
    /// The filtered trace.
    pub trace: Vec<TraceOp>,
    /// Per-call read limit.
    pub max_read: Option<usize>,
    /// Per-call write limit.
    pub max_write: Option<usize>,
}

impl Store {
    /// An empty world.
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            contents: BTreeMap::new(),
            next_fd: 3,
            trace: Vec::new(),
            max_read: None,
            max_write: None,
        }
    }

    /// Reset for one case.
    pub fn reset(&mut self, files: &[Vec<u8>]) {
        self.files.clear();
        self.contents.clear();
        self.next_fd = 3;
        self.trace.clear();
        self.max_read = None;
        self.max_write = None;
        for index in 0..4 {
            let data = files.get(index).cloned().unwrap_or_default();
            self.contents
                .insert(format!("/f{index}").into_bytes(), data);
        }
    }

    fn fd(&mut self) -> i32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        fd
    }

    fn entry(&mut self, fd: i32) -> Result<&mut Entry, i32> {
        self.files.get_mut(&fd).ok_or(-(EBADF as i32))
    }

    /// `read`: return the count or a negative errno.
    pub fn read(&mut self, fd: i32, buf: &mut [u8]) -> Result<usize, i32> {
        let limit = self.max_read.unwrap_or(usize::MAX);
        let want = buf.len().min(limit);
        let entry = self.entry(fd)?;
        let available = entry.data.len().saturating_sub(entry.pos);
        let count = available.min(want);
        if count != 0 {
            buf[..count].copy_from_slice(&entry.data[entry.pos..entry.pos + count]);
            entry.pos += count;
        }
        Ok(count)
    }

    /// `write`: return the count or a negative errno.
    pub fn write(&mut self, fd: i32, chunks: &[&[u8]]) -> Result<usize, i32> {
        let total: usize = chunks.iter().map(|chunk| chunk.len()).sum();
        let limit = self.max_write.unwrap_or(usize::MAX);
        let want = total.min(limit);
        let entry = self.entry(fd)?;
        if entry.fl & 0o2000 != 0 {
            entry.pos = entry.data.len();
        }
        let mut copied = 0usize;
        for chunk in chunks {
            let take = (want - copied).min(chunk.len());
            if take != 0 {
                if entry.pos > entry.data.len() {
                    entry.data.resize(entry.pos, 0);
                }
                if entry.pos + take > entry.data.len() {
                    entry.data.resize(entry.pos + take, 0);
                }
                entry.data[entry.pos..entry.pos + take].copy_from_slice(&chunk[..take]);
                entry.pos += take;
                copied += take;
            }
            if copied == want {
                break;
            }
        }
        Ok(copied)
    }

    /// `seek`.
    pub fn seek(&mut self, fd: i32, off: i64, whence: i32) -> Result<i64, i32> {
        let entry = self.entry(fd)?;
        if entry.kind == Kind::Pipe {
            return Err(-(ESPIPE as i32));
        }
        let base = match whence {
            0 => 0i64,
            1 => entry.pos as i64,
            2 => entry.data.len() as i64,
            _ => return Err(-(EINVAL as i32)),
        };
        let Some(new) = base.checked_add(off) else {
            return Err(-(EINVAL as i32));
        };
        if new < 0 {
            return Err(-(EINVAL as i32));
        }
        entry.pos = new as usize;
        Ok(new)
    }

    /// `close`.
    pub fn close(&mut self, fd: i32) -> Result<(), i32> {
        let entry = self.files.remove(&fd).ok_or(-(EBADF as i32))?;
        if let Some(path) = entry.path {
            self.contents.insert(path, entry.data);
        }
        Ok(())
    }

    /// `open`.
    pub fn open(&mut self, path: &[u8], flags: i32, _mode: u32) -> Result<i32, i32> {
        let create = flags & 0o100 != 0;
        let truncate = flags & 0o1000 != 0;
        let write = flags & 0o2000 != 0;
        let data = match self.contents.get(path) {
            Some(data) => {
                if truncate {
                    Vec::new()
                } else {
                    data.clone()
                }
            }
            None if create => Vec::new(),
            None => return Err(-(ENOENT as i32)),
        };
        if create {
            self.contents.insert(path.to_vec(), data.clone());
        }
        let fd = self.fd();
        let mut fl = flags & 3;
        if write {
            fl |= 0o2000;
        }
        self.files.insert(
            fd,
            Entry {
                data,
                pos: 0,
                fl,
                kind: Kind::Regular,
                path: Some(path.to_vec()),
            },
        );
        self.trace.push(TraceOp {
            op: 0,
            fd,
            len: i64::from(flags),
            result: i64::from(fd),
        });
        Ok(fd)
    }

    /// `fcntl`.
    pub fn fcntl(&mut self, fd: i32, cmd: i32, value: i64) -> Result<i64, i32> {
        let entry = self.entry(fd)?;
        match cmd {
            3 => Ok(i64::from(entry.fl)),
            4 => {
                entry.fl = value as i32;
                Ok(0)
            }
            _ => Ok(0),
        }
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

/// The process-wide world.
pub fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Store::new()))
}

/// The subject environment.
#[derive(Clone, Copy)]
pub struct OracleEnv;

impl SysFile for OracleEnv {
    fn read(&self, fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
        store()
            .lock()
            .unwrap()
            .read(fd, buf)
            .map_err(|error| Errno((-error) as u16))
    }

    fn write(&self, fd: i32, buf: &[u8]) -> Result<usize, Errno> {
        let mut store = store().lock().unwrap();
        let result = store.write(fd, &[buf]);
        if let Ok(count) = result {
            store.trace.push(TraceOp {
                op: 1,
                fd,
                len: buf.len() as i64,
                result: count as i64,
            });
        }
        result.map_err(|error| Errno((-error) as u16))
    }

    fn write2(&self, fd: i32, a: &[u8], b: &[u8]) -> Result<usize, Errno> {
        let mut store = store().lock().unwrap();
        let result = store.write(fd, &[a, b]);
        if let Ok(count) = result {
            store.trace.push(TraceOp {
                op: 1,
                fd,
                len: (a.len() + b.len()) as i64,
                result: count as i64,
            });
        }
        result.map_err(|error| Errno((-error) as u16))
    }

    fn seek(&self, fd: i32, off: i64, whence: i32) -> Result<i64, Errno> {
        let mut store = store().lock().unwrap();
        let result = store.seek(fd, off, whence);
        if let Ok(position) = result {
            store.trace.push(TraceOp {
                op: 2,
                fd,
                len: off,
                result: position,
            });
        }
        result.map_err(|error| Errno((-error) as u16))
    }

    fn close(&self, fd: i32) -> Result<(), Errno> {
        let mut store = store().lock().unwrap();
        let result = store.close(fd);
        if result.is_ok() {
            store.trace.push(TraceOp {
                op: 3,
                fd,
                len: 0,
                result: 0,
            });
        }
        result.map_err(|error| Errno((-error) as u16))
    }

    fn open(&self, path: &[u8], flags: i32, mode: u32) -> Result<i32, Errno> {
        store()
            .lock()
            .unwrap()
            .open(path, flags, mode)
            .map_err(|error| Errno((-error) as u16))
    }

    fn isatty(&self, fd: i32) -> bool {
        store()
            .lock()
            .unwrap()
            .files
            .get(&fd)
            .is_some_and(|entry| entry.kind == Kind::Tty)
    }

    fn fstat(&self, fd: i32) -> Result<StatLite, Errno> {
        let store = store().lock().unwrap();
        match store.files.get(&fd) {
            Some(entry) => Ok(stat_of(entry.kind)),
            None => Err(Errno(EBADF as u16)),
        }
    }

    fn getfl(&self, fd: i32) -> Result<i32, Errno> {
        store()
            .lock()
            .unwrap()
            .files
            .get(&fd)
            .map(|entry| entry.fl)
            .ok_or(Errno(EBADF as u16))
    }

    fn setfl(&self, fd: i32, flags: i32) -> Result<(), Errno> {
        match store().lock().unwrap().files.get_mut(&fd) {
            Some(entry) => {
                entry.fl = flags;
                Ok(())
            }
            None => Err(Errno(EBADF as u16)),
        }
    }

    fn unlink(&self, _path: &[u8]) -> Result<(), Errno> {
        Err(Errno(EINVAL as u16))
    }

    fn rmdir(&self, _path: &[u8]) -> Result<(), Errno> {
        Err(Errno(EINVAL as u16))
    }

    fn rename(&self, _old: &[u8], _new: &[u8]) -> Result<(), Errno> {
        Err(Errno(EINVAL as u16))
    }

    fn pipe2(&self, _flags: i32) -> Result<(i32, i32), Errno> {
        Err(Errno(EINVAL as u16))
    }

    fn random_suffix(&self, out: &mut [u8; 6]) {
        out.copy_from_slice(b"abcdef");
    }
}

/// The `fstat` view of a kind.
pub fn stat_of(kind: Kind) -> StatLite {
    match kind {
        Kind::Regular => StatLite {
            mode: 0o100644,
            blksize: 1024,
            is_char_dev: false,
        },
        Kind::Tty => StatLite {
            mode: 0o020620,
            blksize: 0,
            is_char_dev: true,
        },
        Kind::Pipe => StatLite {
            mode: 0o010600,
            blksize: 0,
            is_char_dev: false,
        },
    }
}

impl Lock for OracleEnv {
    fn tid(&self) -> i32 {
        1
    }

    fn wait(&self, word: &std::sync::atomic::AtomicI32, expected: i32) {
        while word.load(std::sync::atomic::Ordering::Acquire) == expected {
            std::thread::yield_now();
        }
    }

    fn wake(&self, _word: &std::sync::atomic::AtomicI32, _n: i32) {}

    fn threaded(&self) -> bool {
        true
    }
}
