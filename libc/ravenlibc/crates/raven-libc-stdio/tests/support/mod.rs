// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared fake environment of the donor-vector integration tests.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, Ordering};

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EBADF, EINVAL, ENOENT, ESPIPE};
use raven_libc_stdio::{Env, File, Lock, StatLite, State, SysFile};

/// One fake descriptor.
#[derive(Clone)]
pub struct Entry {
    /// Data.
    pub data: Vec<u8>,
    /// Cursor.
    pub pos: usize,
    /// Status flags.
    pub fl: i32,
    /// Tty flag.
    pub tty: bool,
    /// Pipe flag.
    pub pipe: bool,
    /// Path.
    pub path: Option<Vec<u8>>,
}

/// The fake world.
pub struct MemEnv {
    /// Descriptors.
    pub files: Mutex<BTreeMap<i32, Entry>>,
    /// Path contents.
    pub contents: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
    /// Next fd.
    pub next_fd: Mutex<i32>,
    /// Tty descriptors.
    pub tty: Mutex<Vec<i32>>,
}

impl MemEnv {
    /// A fresh world.
    pub fn new() -> Self {
        Self {
            files: Mutex::new(BTreeMap::new()),
            contents: Mutex::new(BTreeMap::new()),
            next_fd: Mutex::new(3),
            tty: Mutex::new(Vec::new()),
        }
    }

    /// Add a path.
    pub fn add(&self, path: &[u8], data: &[u8]) {
        self.contents
            .lock()
            .unwrap()
            .insert(path.to_vec(), data.to_vec());
    }

    /// The contents of a path.
    pub fn bytes(&self, path: &[u8]) -> Vec<u8> {
        self.contents
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or_default()
    }

    /// The data of a descriptor.
    pub fn data(&self, fd: i32) -> Vec<u8> {
        self.files
            .lock()
            .unwrap()
            .get(&fd)
            .map(|entry| entry.data.clone())
            .unwrap_or_default()
    }

    /// Add a descriptor.
    pub fn push(&self, data: &[u8], tty: bool, pipe: bool) -> i32 {
        let mut next = self.next_fd.lock().unwrap();
        let fd = *next;
        *next += 1;
        self.files.lock().unwrap().insert(
            fd,
            Entry {
                data: data.to_vec(),
                pos: 0,
                fl: 0,
                tty,
                pipe,
                path: None,
            },
        );
        if tty {
            self.tty.lock().unwrap().push(fd);
        }
        fd
    }

    /// Install fds 0, 1 and 2 (tty or regular).
    pub fn install_std(&self, tty: bool) {
        let mut files = self.files.lock().unwrap();
        for fd in 0..3 {
            files.insert(
                fd,
                Entry {
                    data: Vec::new(),
                    pos: 0,
                    fl: 1,
                    tty,
                    pipe: false,
                    path: None,
                },
            );
        }
        if tty {
            let mut tty_fds = self.tty.lock().unwrap();
            tty_fds.extend_from_slice(&[0, 1, 2]);
        }
    }

    /// A state over this environment.
    pub fn state(self) -> &'static State<Self> {
        Box::leak(Box::new(State::new(self)))
    }
}

impl SysFile for MemEnv {
    fn read(&self, fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
        let mut files = self.files.lock().unwrap();
        let entry = files.get_mut(&fd).ok_or(Errno(EBADF as u16))?;
        let available = entry.data.len().saturating_sub(entry.pos);
        let count = available.min(buf.len());
        if count != 0 {
            buf[..count].copy_from_slice(&entry.data[entry.pos..entry.pos + count]);
            entry.pos += count;
        }
        Ok(count)
    }

    fn write(&self, fd: i32, buf: &[u8]) -> Result<usize, Errno> {
        let mut files = self.files.lock().unwrap();
        let entry = files.get_mut(&fd).ok_or(Errno(EBADF as u16))?;
        if entry.fl & 0o2000 != 0 {
            entry.pos = entry.data.len();
        }
        let count = buf.len();
        if entry.pos > entry.data.len() {
            entry.data.resize(entry.pos, 0);
        }
        if entry.pos + count > entry.data.len() {
            entry.data.resize(entry.pos + count, 0);
        }
        entry.data[entry.pos..entry.pos + count].copy_from_slice(buf);
        entry.pos += count;
        if let Some(path) = entry.path.clone() {
            self.contents
                .lock()
                .unwrap()
                .insert(path, entry.data.clone());
        }
        Ok(count)
    }

    fn write2(&self, fd: i32, a: &[u8], b: &[u8]) -> Result<usize, Errno> {
        let mut joined = Vec::with_capacity(a.len() + b.len());
        joined.extend_from_slice(a);
        joined.extend_from_slice(b);
        self.write(fd, &joined)
    }

    fn seek(&self, fd: i32, off: i64, whence: i32) -> Result<i64, Errno> {
        let mut files = self.files.lock().unwrap();
        let entry = files.get_mut(&fd).ok_or(Errno(EBADF as u16))?;
        if entry.pipe {
            return Err(Errno(ESPIPE as u16));
        }
        let base = match whence {
            0 => 0i64,
            1 => entry.pos as i64,
            2 => entry.data.len() as i64,
            _ => return Err(Errno(EINVAL as u16)),
        };
        let new = base + off;
        if new < 0 {
            return Err(Errno(EINVAL as u16));
        }
        entry.pos = new as usize;
        Ok(new)
    }

    fn close(&self, fd: i32) -> Result<(), Errno> {
        let mut files = self.files.lock().unwrap();
        let entry = files.remove(&fd).ok_or(Errno(EBADF as u16))?;
        if let Some(path) = entry.path {
            self.contents.lock().unwrap().insert(path, entry.data);
        }
        Ok(())
    }

    fn open(&self, path: &[u8], flags: i32, _mode: u32) -> Result<i32, Errno> {
        let create = flags & 0o100 != 0;
        let truncate = flags & 0o1000 != 0;
        let data = match self.contents.lock().unwrap().get(path) {
            Some(data) => {
                if truncate {
                    Vec::new()
                } else {
                    data.clone()
                }
            }
            None if create => Vec::new(),
            None => return Err(Errno(ENOENT as u16)),
        };
        if create {
            self.contents
                .lock()
                .unwrap()
                .insert(path.to_vec(), data.clone());
        }
        let mut next = self.next_fd.lock().unwrap();
        let fd = *next;
        *next += 1;
        let mut fl = flags & 3;
        if flags & 0o2000 != 0 {
            fl |= 0o2000;
        }
        self.files.lock().unwrap().insert(
            fd,
            Entry {
                data,
                pos: 0,
                fl,
                tty: false,
                pipe: false,
                path: Some(path.to_vec()),
            },
        );
        Ok(fd)
    }

    fn isatty(&self, fd: i32) -> bool {
        self.files
            .lock()
            .unwrap()
            .get(&fd)
            .is_some_and(|entry| entry.tty)
    }

    fn fstat(&self, fd: i32) -> Result<StatLite, Errno> {
        let files = self.files.lock().unwrap();
        let entry = files.get(&fd).ok_or(Errno(EBADF as u16))?;
        Ok(StatLite {
            mode: if entry.tty { 0o020620 } else { 0o100644 },
            blksize: if entry.tty { 0 } else { 1024 },
            is_char_dev: entry.tty,
        })
    }

    fn getfl(&self, fd: i32) -> Result<i32, Errno> {
        self.files
            .lock()
            .unwrap()
            .get(&fd)
            .map(|entry| entry.fl)
            .ok_or(Errno(EBADF as u16))
    }

    fn setfl(&self, fd: i32, flags: i32) -> Result<(), Errno> {
        match self.files.lock().unwrap().get_mut(&fd) {
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
        Ok((self.push(b"", false, true), self.push(b"", false, true)))
    }

    fn random_suffix(&self, out: &mut [u8; 6]) {
        out.copy_from_slice(b"abcdef");
    }
}

impl Lock for MemEnv {
    fn tid(&self) -> i32 {
        1
    }

    fn wait(&self, word: &AtomicI32, expected: i32) {
        while word.load(Ordering::Acquire) == expected {
            std::thread::yield_now();
        }
    }

    fn wake(&self, _word: &AtomicI32, _n: i32) {}

    fn threaded(&self) -> bool {
        true
    }
}

/// A fresh state.
pub fn state() -> (&'static State<MemEnv>, &'static MemEnv) {
    let env = MemEnv::new();
    let state: &'static State<MemEnv> = Box::leak(Box::new(State::new(env)));
    (state, state.env())
}

/// Open a path and return the FILE.
pub fn open(state: &'static State<MemEnv>, path: &[u8], mode: &[u8]) -> &'static File<MemEnv> {
    let pointer = raven_libc_stdio::fopen(state, path, mode).expect("fopen");
    raven_libc_stdio::file_from_ptr(pointer)
}

/// An empty caller FILE.
pub fn empty_file(state: &'static State<MemEnv>) -> &'static File<MemEnv> {
    &*Box::leak(Box::new(File::new(state)))
}

/// The `Env` blanket impl is the only bound the crate needs.
pub fn assert_env<T: Env>() {}
