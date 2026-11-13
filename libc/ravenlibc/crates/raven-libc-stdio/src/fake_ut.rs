// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The fake environment of the unit tests: an in-memory descriptor table, a scripted
//! [`Lock`] over `std::sync`, failure injection and a call trace.
//!
//! This file is compiled only under `cfg(test)`; it is test code and never appears in the
//! shipped crate (`donor-map/04` 4.5). The kinds cover the three shapes the FILE layer cares
//! about: a seekable regular file with a configurable `st_blksize`, a character-device tty
//! (`isatty == true`), and an unseekable pipe (`ESPIPE`).

#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::bool_assert_comparison,
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread_local;
use std::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EBADF, EINVAL, EIO, EISDIR, ENOENT, ENOSPC, ESPIPE};

use crate::traits::{Lock, StatLite, SysFile};

/// The kind of one fake descriptor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A seekable regular file with this `st_blksize`.
    Regular(u32),
    /// A character device that is a terminal.
    Tty,
    /// An unseekable pipe.
    Pipe,
}

/// One recorded operation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Op {
    /// `read` with the requested length and the result.
    Read { fd: i32, len: usize, ret: isize },
    /// `write` with the requested length and the result.
    Write { fd: i32, len: usize, ret: isize },
    /// `write2` with the combined length and the result.
    Write2 { fd: i32, len: usize, ret: isize },
    /// `seek`.
    Seek {
        fd: i32,
        off: i64,
        whence: i32,
        ret: i64,
    },
    /// `close`.
    Close { fd: i32 },
    /// `open`.
    Open { path: Vec<u8>, flags: i32 },
    /// `fstat`.
    Fstat { fd: i32 },
    /// `isatty`.
    Isatty { fd: i32, tty: bool },
    /// `fcntl(F_GETFL)`.
    Getfl { fd: i32 },
    /// `fcntl(F_SETFL)`.
    Setfl { fd: i32, flags: i32 },
    /// `pipe2`.
    Pipe2,
}

/// One fake descriptor.
#[derive(Clone)]
pub struct FakeFile {
    /// The data.
    pub data: Vec<u8>,
    /// The cursor.
    pub pos: usize,
    /// The `O_*` status flags.
    pub fl: i32,
    /// The kind.
    pub kind: Kind,
    /// The path this fd was opened from, if any.
    pub path: Option<Vec<u8>>,
}

/// The mutable world.
pub struct FakeInner {
    /// Open descriptors.
    pub files: BTreeMap<i32, FakeFile>,
    /// Path contents.
    pub contents: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Directories.
    pub dirs: BTreeSet<Vec<u8>>,
    /// Buffer-size hints per path.
    pub block_sizes: BTreeMap<Vec<u8>, u32>,
    /// Next descriptor.
    pub next_fd: i32,
    /// The trace.
    pub trace: Vec<Op>,
    /// Per-call read limit.
    pub max_read: Option<usize>,
    /// Per-call write limit.
    pub max_write: Option<usize>,
    /// Fail the write whose index (1-based) is this.
    pub fail_write_at: Option<usize>,
    /// Return `EINTR` from the read whose index (1-based) is this.
    pub eintr_read_at: Option<usize>,
    /// Fail the next `open` with `ENOMEM`.
    pub fail_open: bool,
    /// The number of writes so far.
    pub writes: usize,
    /// The number of reads so far.
    pub reads: usize,
    /// The suffix counter of `random_suffix`.
    pub suffix: u8,
}

impl FakeInner {
    fn fd(&mut self) -> i32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        fd
    }

    fn get(&self, fd: i32) -> Result<&FakeFile, Errno> {
        self.files.get(&fd).ok_or(Errno(EBADF as u16))
    }

    fn get_mut(&mut self, fd: i32) -> Result<&mut FakeFile, Errno> {
        self.files.get_mut(&fd).ok_or(Errno(EBADF as u16))
    }
}

/// The fake environment.
pub struct FakeEnv {
    /// The mutable world.
    pub inner: Mutex<FakeInner>,
    wait_mutex: Mutex<()>,
    wait_cond: Condvar,
    threaded: AtomicBool,
}

thread_local! {
    static TID: i32 = next_tid();
}

static NEXT_TID: AtomicI32 = AtomicI32::new(1);

fn next_tid() -> i32 {
    NEXT_TID.fetch_add(1, Ordering::Relaxed)
}

impl Default for FakeEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeEnv {
    /// An empty world.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(FakeInner {
                files: BTreeMap::new(),
                contents: BTreeMap::new(),
                dirs: BTreeSet::new(),
                block_sizes: BTreeMap::new(),
                next_fd: 3,
                trace: Vec::new(),
                max_read: None,
                max_write: None,
                fail_write_at: None,
                eintr_read_at: None,
                fail_open: false,
                writes: 0,
                reads: 0,
                suffix: b'a',
            }),
            wait_mutex: Mutex::new(()),
            wait_cond: Condvar::new(),
            threaded: AtomicBool::new(true),
        }
    }

    /// Add a directory.
    pub fn add_dir(&self, path: &[u8]) {
        self.inner.lock().unwrap().dirs.insert(path.to_vec());
    }

    /// Add a path with contents.
    pub fn add_file(&self, path: &[u8], data: &[u8], blksize: u32) {
        let mut inner = self.inner.lock().unwrap();
        inner.contents.insert(path.to_vec(), data.to_vec());
        inner.block_sizes.insert(path.to_vec(), blksize);
    }

    /// Install one descriptor with explicit contents.
    pub fn push_fd(&self, data: &[u8], kind: Kind) -> i32 {
        let mut inner = self.inner.lock().unwrap();
        let fd = inner.fd();
        inner.files.insert(
            fd,
            FakeFile {
                data: data.to_vec(),
                pos: 0,
                fl: 0,
                kind,
                path: None,
            },
        );
        fd
    }

    /// Install the tty as fd 1 and a regular file as fd 0.
    pub fn install_std_tty(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.files.insert(
            0,
            FakeFile {
                data: Vec::new(),
                pos: 0,
                fl: 0,
                kind: Kind::Tty,
                path: None,
            },
        );
        inner.files.insert(
            1,
            FakeFile {
                data: Vec::new(),
                pos: 0,
                fl: 1,
                kind: Kind::Tty,
                path: None,
            },
        );
        inner.files.insert(
            2,
            FakeFile {
                data: Vec::new(),
                pos: 0,
                fl: 1,
                kind: Kind::Tty,
                path: None,
            },
        );
    }

    /// Install a non-tty regular file on fds 0, 1 and 2.
    pub fn install_std_file(&self) {
        let mut inner = self.inner.lock().unwrap();
        for fd in 0..3 {
            inner.files.insert(
                fd,
                FakeFile {
                    data: Vec::new(),
                    pos: 0,
                    fl: 1,
                    kind: Kind::Regular(1024),
                    path: None,
                },
            );
        }
    }

    /// The contents of a path.
    pub fn contents(&self, path: &[u8]) -> Vec<u8> {
        self.inner
            .lock()
            .unwrap()
            .contents
            .get(path)
            .cloned()
            .unwrap_or_default()
    }

    /// The data of one descriptor.
    pub fn data(&self, fd: i32) -> Vec<u8> {
        self.inner
            .lock()
            .unwrap()
            .files
            .get(&fd)
            .map(|file| file.data.clone())
            .unwrap_or_default()
    }

    /// The trace.
    pub fn trace(&self) -> Vec<Op> {
        self.inner.lock().unwrap().trace.clone()
    }

    /// Clear the trace.
    pub fn clear_trace(&self) {
        self.inner.lock().unwrap().trace.clear();
    }

    /// Limit every read.
    pub fn set_max_read(&self, limit: usize) {
        self.inner.lock().unwrap().max_read = Some(limit);
    }

    /// Limit every write.
    pub fn set_max_write(&self, limit: usize) {
        self.inner.lock().unwrap().max_write = Some(limit);
    }

    /// Fail the `n`-th write (1-based).
    pub fn fail_write_after(&self, n: usize) {
        self.inner.lock().unwrap().fail_write_at = Some(n);
    }

    /// Return `EINTR` from the `n`-th read (1-based).
    pub fn eintr_on(&self, n: usize) {
        self.inner.lock().unwrap().eintr_read_at = Some(n);
    }

    /// Fail the next `open` with `ENOMEM`.
    pub fn fail_next_open(&self) {
        self.inner.lock().unwrap().fail_open = true;
    }

    /// Set the single-thread vs threaded answer.
    pub fn set_threaded(&self, value: bool) {
        self.threaded.store(value, Ordering::Release);
    }

    /// The current tid.
    pub fn tid(&self) -> i32 {
        TID.with(|tid| *tid)
    }
}

impl SysFile for FakeEnv {
    fn read(&self, fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
        let mut inner = self.inner.lock().unwrap();
        inner.reads += 1;
        if inner.eintr_read_at == Some(inner.reads) {
            inner.trace.push(Op::Read {
                fd,
                len: buf.len(),
                ret: -(EINVAL as isize),
            });
            return Err(Errno(EINVAL as u16));
        }
        let limit = inner.max_read.unwrap_or(usize::MAX);
        let want = buf.len().min(limit);
        let file = inner.get_mut(fd)?;
        let count = match file.kind {
            Kind::Regular(_) | Kind::Tty => {
                let available = file.data.len().saturating_sub(file.pos);
                let count = available.min(want);
                if count != 0 {
                    buf[..count].copy_from_slice(&file.data[file.pos..file.pos + count]);
                    file.pos += count;
                }
                count
            }
            Kind::Pipe => {
                let available = file.data.len().saturating_sub(file.pos);
                let count = available.min(want);
                if count != 0 {
                    buf[..count].copy_from_slice(&file.data[file.pos..file.pos + count]);
                    file.pos += count;
                }
                count
            }
        };
        let ret = count as isize;
        inner.trace.push(Op::Read {
            fd,
            len: buf.len(),
            ret,
        });
        Ok(count)
    }

    fn write(&self, fd: i32, buf: &[u8]) -> Result<usize, Errno> {
        let mut inner = self.inner.lock().unwrap();
        inner.writes += 1;
        if inner.fail_write_at == Some(inner.writes) {
            inner.trace.push(Op::Write {
                fd,
                len: buf.len(),
                ret: -(EIO as isize),
            });
            return Err(Errno(ENOSPC as u16));
        }
        let limit = inner.max_write.unwrap_or(usize::MAX);
        let want = buf.len().min(limit);
        let file = inner.get_mut(fd)?;
        if file.fl & 0o2000 != 0 {
            file.pos = file.data.len();
        }
        let count = match file.kind {
            Kind::Pipe => {
                let count = want;
                file.data.extend_from_slice(&buf[..count]);
                count
            }
            _ => {
                let count = want;
                if file.pos > file.data.len() {
                    file.data.resize(file.pos, 0);
                }
                if file.pos + count > file.data.len() {
                    file.data.resize(file.pos + count, 0);
                }
                file.data[file.pos..file.pos + count].copy_from_slice(&buf[..count]);
                file.pos += count;
                count
            }
        };
        let path = file.path.clone();
        let image = file.data.clone();
        let ret = count as isize;
        inner.trace.push(Op::Write {
            fd,
            len: buf.len(),
            ret,
        });
        if let Some(path) = path {
            inner.contents.insert(path, image);
        }
        Ok(count)
    }

    fn write2(&self, fd: i32, a: &[u8], b: &[u8]) -> Result<usize, Errno> {
        let mut inner = self.inner.lock().unwrap();
        inner.writes += 1;
        if inner.fail_write_at == Some(inner.writes) {
            inner.trace.push(Op::Write2 {
                fd,
                len: a.len() + b.len(),
                ret: -(EIO as isize),
            });
            return Err(Errno(ENOSPC as u16));
        }
        let limit = inner.max_write.unwrap_or(usize::MAX);
        let total = a.len() + b.len();
        let want = total.min(limit);
        let file = inner.get_mut(fd)?;
        let mut copied = 0usize;
        for slice in [a, b] {
            let take = (want - copied).min(slice.len());
            if take != 0 {
                if file.pos > file.data.len() {
                    file.data.resize(file.pos, 0);
                }
                if file.pos + take > file.data.len() {
                    file.data.resize(file.pos + take, 0);
                }
                file.data[file.pos..file.pos + take].copy_from_slice(&slice[..take]);
                file.pos += take;
                copied += take;
            }
            if copied == want {
                break;
            }
        }
        let path = file.path.clone();
        let image = file.data.clone();
        inner.trace.push(Op::Write2 {
            fd,
            len: total,
            ret: copied as isize,
        });
        if let Some(path) = path {
            inner.contents.insert(path, image);
        }
        Ok(copied)
    }

    fn seek(&self, fd: i32, off: i64, whence: i32) -> Result<i64, Errno> {
        let mut inner = self.inner.lock().unwrap();
        let file = inner.get_mut(fd)?;
        if file.kind == Kind::Pipe {
            inner.trace.push(Op::Seek {
                fd,
                off,
                whence,
                ret: -(ESPIPE as i64),
            });
            return Err(Errno(ESPIPE as u16));
        }
        let base = match whence {
            0 => 0i64,
            1 => file.pos as i64,
            2 => file.data.len() as i64,
            _ => return Err(Errno(EINVAL as u16)),
        };
        let new = base.checked_add(off).ok_or(Errno(EINVAL as u16))?;
        if new < 0 {
            return Err(Errno(EINVAL as u16));
        }
        file.pos = new as usize;
        inner.trace.push(Op::Seek {
            fd,
            off,
            whence,
            ret: new,
        });
        Ok(new)
    }

    fn close(&self, fd: i32) -> Result<(), Errno> {
        let mut inner = self.inner.lock().unwrap();
        let file = inner.files.remove(&fd).ok_or(Errno(EBADF as u16))?;
        if let Some(path) = file.path {
            inner.contents.insert(path, file.data);
        }
        inner.trace.push(Op::Close { fd });
        Ok(())
    }

    fn open(&self, path: &[u8], flags: i32, _mode: u32) -> Result<i32, Errno> {
        let mut inner = self.inner.lock().unwrap();
        if inner.fail_open {
            inner.fail_open = false;
            return Err(Errno(ENOSPC as u16));
        }
        let create = flags & 0o100 != 0;
        let truncate = flags & 0o1000 != 0;
        let existing = inner.contents.get(path).cloned();
        let data = match existing {
            Some(data) => {
                if truncate {
                    Vec::new()
                } else {
                    data
                }
            }
            None if create => Vec::new(),
            None => return Err(Errno(ENOENT as u16)),
        };
        if create {
            inner.contents.insert(path.to_vec(), data.clone());
        }
        let fd = inner.fd();
        let access = (flags & 3) | (flags & 0o2000);
        let blksize = inner.block_sizes.get(path).copied().unwrap_or(1024);
        inner.files.insert(
            fd,
            FakeFile {
                data,
                pos: 0,
                fl: access,
                kind: Kind::Regular(blksize),
                path: Some(path.to_vec()),
            },
        );
        inner.trace.push(Op::Open {
            path: path.to_vec(),
            flags,
        });
        Ok(fd)
    }

    fn isatty(&self, fd: i32) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let tty = inner
            .files
            .get(&fd)
            .map(|file| file.kind == Kind::Tty)
            .unwrap_or(false);
        inner.trace.push(Op::Isatty { fd, tty });
        tty
    }

    fn fstat(&self, fd: i32) -> Result<StatLite, Errno> {
        let mut inner = self.inner.lock().unwrap();
        let file = inner.get(fd)?.clone();
        inner.trace.push(Op::Fstat { fd });
        Ok(match file.kind {
            Kind::Regular(blksize) => StatLite {
                mode: 0o100644,
                blksize,
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
        })
    }

    fn getfl(&self, fd: i32) -> Result<i32, Errno> {
        let mut inner = self.inner.lock().unwrap();
        let flags = inner.get(fd)?.fl;
        inner.trace.push(Op::Getfl { fd });
        Ok(flags)
    }

    fn setfl(&self, fd: i32, flags: i32) -> Result<(), Errno> {
        let mut inner = self.inner.lock().unwrap();
        let file = inner.get_mut(fd)?;
        file.fl = flags;
        inner.trace.push(Op::Setfl { fd, flags });
        Ok(())
    }

    fn unlink(&self, path: &[u8]) -> Result<(), Errno> {
        let mut inner = self.inner.lock().unwrap();
        if inner.dirs.contains(path) {
            return Err(Errno(EISDIR as u16));
        }
        match inner.contents.remove(path) {
            Some(_) => Ok(()),
            None => Err(Errno(ENOENT as u16)),
        }
    }

    fn rmdir(&self, path: &[u8]) -> Result<(), Errno> {
        let mut inner = self.inner.lock().unwrap();
        if inner.dirs.remove(path) {
            Ok(())
        } else {
            Err(Errno(ENOENT as u16))
        }
    }

    fn rename(&self, old: &[u8], new: &[u8]) -> Result<(), Errno> {
        let mut inner = self.inner.lock().unwrap();
        match inner.contents.remove(old) {
            Some(data) => {
                inner.contents.insert(new.to_vec(), data);
                Ok(())
            }
            None => Err(Errno(ENOENT as u16)),
        }
    }

    fn pipe2(&self, _flags: i32) -> Result<(i32, i32), Errno> {
        let mut inner = self.inner.lock().unwrap();
        let a = inner.fd();
        let b = inner.fd();
        let file = || FakeFile {
            data: Vec::new(),
            pos: 0,
            fl: 0,
            kind: Kind::Pipe,
            path: None,
        };
        inner.files.insert(a, file());
        inner.files.insert(b, file());
        if let Some(write_end) = inner.files.get_mut(&b) {
            write_end.fl = 1;
        }
        inner.trace.push(Op::Pipe2);
        Ok((a, b))
    }

    fn random_suffix(&self, out: &mut [u8; 6]) {
        let mut inner = self.inner.lock().unwrap();
        for slot in out.iter_mut() {
            *slot = inner.suffix;
            inner.suffix = if inner.suffix == b'z' {
                b'a'
            } else {
                inner.suffix + 1
            };
        }
    }
}

impl Lock for FakeEnv {
    fn tid(&self) -> i32 {
        TID.with(|tid| *tid)
    }

    fn wait(&self, word: &AtomicI32, expected: i32) {
        let guard = self.wait_mutex.lock().unwrap();
        if word.load(Ordering::Acquire) != expected {
            return;
        }
        let _ = self
            .wait_cond
            .wait_timeout(guard, std::time::Duration::from_millis(1))
            .unwrap();
    }

    fn wake(&self, _word: &AtomicI32, _n: i32) {
        self.wait_cond.notify_all();
    }

    fn threaded(&self) -> bool {
        self.threaded.load(Ordering::Acquire)
    }
}

thread_local! {
    static STRING_POOL: std::cell::RefCell<Vec<Vec<u8>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A minimal argument environment for the printf/scanf glue tests.
#[derive(Clone, Default)]
pub struct EngineArgs {
    /// Integer words in call order.
    pub words: Vec<u64>,
    /// Double bits in call order.
    pub floats: Vec<u64>,
    /// Long-double bits in call order.
    pub longs: Vec<u128>,
    /// Recorded `store_int` values.
    pub stored_int: Vec<u64>,
    /// Recorded `store_chars` bytes.
    pub stored_chars: Vec<u8>,
    /// Recorded `%n` value.
    pub count: Option<i64>,
}

impl EngineArgs {
    /// Add a C string to the thread-local pool and return its address.
    pub fn cstring(&mut self, text: &[u8]) -> usize {
        STRING_POOL.with(|pool| {
            let mut pool = pool.borrow_mut();
            let mut data = text.to_vec();
            data.push(0);
            pool.push(data);
            pool.last().map(|data| data.as_ptr() as usize).unwrap_or(0)
        })
    }
}

impl raven_libc_fmt::VaReader for EngineArgs {
    fn next_u64(&mut self) -> u64 {
        if self.words.is_empty() {
            0
        } else {
            self.words.remove(0)
        }
    }

    fn next_f64_bits(&mut self) -> u64 {
        if self.floats.is_empty() {
            0
        } else {
            self.floats.remove(0)
        }
    }

    fn next_ld_bits(&mut self) -> u128 {
        if self.longs.is_empty() {
            0
        } else {
            self.longs.remove(0)
        }
    }

    fn clone_state(&self) -> Self {
        self.clone()
    }
}

impl raven_libc_fmt::FmtEnv for EngineArgs {
    fn cstr_len(&mut self, ptr: usize, max: usize) -> usize {
        STRING_POOL.with(|pool| {
            for string in pool.borrow().iter() {
                if string.as_ptr() as usize == ptr {
                    let len = string
                        .iter()
                        .position(|byte| *byte == 0)
                        .unwrap_or(string.len());
                    return len.min(max);
                }
            }
            0
        })
    }

    fn read_bytes(&mut self, ptr: usize, offset: usize, buf: &mut [u8]) -> usize {
        STRING_POOL.with(|pool| {
            for string in pool.borrow().iter() {
                if string.as_ptr() as usize == ptr {
                    let available = string.len().saturating_sub(offset);
                    let count = available.min(buf.len());
                    buf[..count].copy_from_slice(&string[offset..offset + count]);
                    return count;
                }
            }
            0
        })
    }

    fn wstr_len(&mut self, _ptr: usize, _max: usize) -> usize {
        0
    }

    fn read_wchars(&mut self, _ptr: usize, _offset: usize, _buf: &mut [u32]) -> usize {
        0
    }

    fn store_count(&mut self, _ptr: usize, _size: raven_libc_fmt::StoreSize, value: i64) {
        self.count = Some(value);
    }

    fn errno_text(&mut self, _errnum: i32, buf: &mut [u8; 64]) -> usize {
        let text = b"errno-text";
        let count = text.len().min(buf.len());
        buf[..count].copy_from_slice(&text[..count]);
        count
    }

    fn store_int(&mut self, _ptr: usize, _size: raven_libc_fmt::StoreSize, value: u64) {
        self.stored_int.push(value);
    }

    fn store_chars(&mut self, _ptr: usize, data: &[u8]) {
        self.stored_chars.extend_from_slice(data);
    }

    fn store_wchars(&mut self, _ptr: usize, _data: &[u32]) {}

    fn store_float_bits(&mut self, _ptr: usize, _kind: raven_libc_fmt::FloatKind, _bits: u128) {}

    fn alloc_store(&mut self, _dest_ptr: usize, bytes: &[u8], _wide: bool) -> bool {
        self.stored_chars.extend_from_slice(bytes);
        true
    }
}

/// A leaked state with a fresh fake environment.
pub fn leaked_state() -> &'static crate::state::State<FakeEnv> {
    std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(
        FakeEnv::new(),
    )))
}

/// A leaked empty FILE.
pub fn leaked_file(
    state: &'static crate::state::State<FakeEnv>,
) -> &'static crate::file::File<FakeEnv> {
    &*std::boxed::Box::leak(std::boxed::Box::new(crate::file::File::new(state)))
}

/// A leaked [`crate::std_streams::StdStreams`].
pub fn leaked_streams(
    state: &'static crate::state::State<FakeEnv>,
) -> &'static crate::std_streams::StdStreams<FakeEnv> {
    std::boxed::Box::leak(std::boxed::Box::new(crate::std_streams::StdStreams::new(
        state,
    )))
}

/// Open a path and return the FILE (the test owns no pointer).
pub fn open_file(
    state: &'static crate::state::State<FakeEnv>,
    path: &[u8],
    mode: &[u8],
) -> &'static crate::file::File<FakeEnv> {
    let pointer = crate::open::fopen(state, path, mode).expect("fopen");
    crate::raw::file_ref(pointer)
}

/// The lock-order detector of the fake: a list lock taken while a FILE lock is held is an
/// inversion (`donor-map/09` 3.8: group 5 before group 6).
pub mod order {
    use std::cell::RefCell;

    thread_local! {
        static STATE: RefCell<(usize, usize)> = const { RefCell::new((0, 0)) };
        static INVERSIONS: RefCell<usize> = const { RefCell::new(0) };
    }

    /// A FILE lock was acquired.
    pub fn file_acquired() {
        STATE.with(|state| state.borrow_mut().0 += 1);
    }

    /// A FILE lock was released.
    pub fn file_released() {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.0 = state.0.saturating_sub(1);
        });
    }

    /// The list lock is being acquired.
    pub fn list_acquired() {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.0 != 0 {
                INVERSIONS.with(|count| *count.borrow_mut() += 1);
            }
            state.1 += 1;
        });
    }

    /// The list lock was released.
    pub fn list_released() {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.1 = state.1.saturating_sub(1);
        });
    }

    /// The number of detected inversions.
    pub fn inversions() -> usize {
        INVERSIONS.with(|count| *count.borrow())
    }

    /// Reset the detector.
    pub fn reset() {
        STATE.with(|state| *state.borrow_mut() = (0, 0));
        INVERSIONS.with(|count| *count.borrow_mut() = 0);
    }
}
