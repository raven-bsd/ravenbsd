// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic fakes for every seam (ticket C65 section 4.2).
//!
//! Test-only: the module is compiled under `#[cfg(test)]` and is never part of a shipped
//! artifact. The integration tests under `tests/` carry their own copies because `cfg(test)`
//! items are not visible to them.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use raven_libc_abi::errno::Errno;

use crate::seams::{
    Entropy, ForkResult, LineSource, PidfileOps, PtyOps, ReadFile, Termios, TtyOps,
};

/// A deterministic xorshift64* generator: the same seed produces the same bytes on every run.
#[derive(Clone, Debug)]
pub struct FakeEntropy {
    state: u64,
    draws: usize,
}

impl FakeEntropy {
    /// A generator with the given seed; seed 0 is mapped to 1.
    pub const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
            draws: 0,
        }
    }

    /// How many `fill` calls happened.
    pub const fn draws(&self) -> usize {
        self.draws
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
}

impl Entropy for FakeEntropy {
    fn fill(&mut self, buf: &mut [u8]) {
        self.draws += 1;
        let mut offset = 0;
        while offset < buf.len() {
            let word = self.next_u64().to_ne_bytes();
            let take = core::cmp::min(8, buf.len() - offset);
            buf[offset..offset + take].copy_from_slice(&word[..take]);
            offset += take;
        }
    }
}

/// A `ReadFile` over an in-memory map with a call log (used to prove no `.db` probe happens).
#[derive(Clone, Debug, Default)]
pub struct FakeFiles {
    files: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Every path passed to `read_all`, in order.
    pub log: Vec<Vec<u8>>,
    /// When set, `read_all` fails with this errno instead of consulting `files`.
    pub fail_with: Option<Errno>,
}

impl FakeFiles {
    /// Empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace a file.
    pub fn insert(&mut self, path: &[u8], contents: &[u8]) {
        self.files.insert(path.to_vec(), contents.to_vec());
    }
}

impl ReadFile for FakeFiles {
    fn read_all(&mut self, path: &[u8], out: &mut Vec<u8>) -> Result<(), Errno> {
        self.log.push(path.to_vec());
        out.clear();
        if let Some(error) = self.fail_with {
            return Err(error);
        }
        match self.files.get(path) {
            Some(data) => {
                if data.len() > crate::seams::READ_FILE_CAP {
                    return Err(Errno(raven_libc_abi::errno::ERANGE as u16));
                }
                out.extend_from_slice(data);
                Ok(())
            }
            None => Err(Errno(raven_libc_abi::errno::ENOENT as u16)),
        }
    }
}

/// A `LineSource` over an in-memory list of physical lines.
#[derive(Clone, Debug, Default)]
pub struct FakeLines {
    lines: Vec<Vec<u8>>,
    cursor: usize,
}

impl FakeLines {
    /// Build from logical strings, appending `\n` to each one.
    pub fn from_text(text: &str) -> Self {
        let mut lines = Vec::new();
        for line in text.split_inclusive('\n') {
            lines.push(line.as_bytes().to_vec());
        }
        Self { lines, cursor: 0 }
    }
}

impl LineSource for FakeLines {
    fn next_line(&mut self) -> Option<Vec<u8>> {
        let line = self.lines.get(self.cursor).cloned();
        if line.is_some() {
            self.cursor += 1;
        }
        line
    }
}

/// A `PidfileOps` that records every operation.
#[derive(Clone, Debug)]
pub struct FakePid {
    /// Every `create_write` call.
    pub writes: Vec<(Vec<u8>, Vec<u8>)>,
    /// Every `unlink` call.
    pub unlinks: Vec<Vec<u8>>,
    /// Every registered `atexit` function.
    pub atexits: Vec<fn()>,
    /// The pid this fake reports.
    pub pid: i32,
    /// When set, `create_write` fails with this errno.
    pub write_error: Option<Errno>,
    /// When set, `atexit` fails with this errno.
    pub atexit_error: Option<Errno>,
}

impl Default for FakePid {
    fn default() -> Self {
        Self {
            writes: Vec::new(),
            unlinks: Vec::new(),
            atexits: Vec::new(),
            pid: 4242,
            write_error: None,
            atexit_error: None,
        }
    }
}

impl PidfileOps for FakePid {
    fn create_write(&mut self, path: &[u8], contents: &[u8]) -> Result<(), Errno> {
        if let Some(error) = self.write_error {
            return Err(error);
        }
        self.writes.push((path.to_vec(), contents.to_vec()));
        Ok(())
    }

    fn unlink(&mut self, path: &[u8]) {
        self.unlinks.push(path.to_vec());
    }

    fn getpid(&self) -> i32 {
        self.pid
    }

    fn atexit(&mut self, f: fn()) -> Result<(), Errno> {
        if let Some(error) = self.atexit_error {
            return Err(error);
        }
        self.atexits.push(f);
        Ok(())
    }
}

/// A scripted `TtyOps`: bytes to deliver, termios state and a signal queue.
#[derive(Clone, Debug)]
pub struct FakeTty {
    /// Bytes the terminal returns, in order.
    pub input: Vec<u8>,
    /// Current terminal attributes.
    pub termios: Termios,
    /// Signals waiting to be observed.
    pub signals: Vec<i32>,
    /// Set when `open_tty` fails.
    pub open_error: Option<Errno>,
    /// Set when `tcgetattr` fails (no tty attributes).
    pub tcgetattr_error: Option<Errno>,
    /// Every `write` call, in order.
    pub writes: Vec<(i32, Vec<u8>)>,
    /// Every `tcsetattr` call's `c_lflag`.
    pub lflag_log: Vec<u32>,
    /// Number of `close` calls.
    pub closes: usize,
    /// Descriptor returned by `open_tty`.
    pub tty_fd: i32,
}

impl Default for FakeTty {
    fn default() -> Self {
        Self {
            input: Vec::new(),
            termios: Termios {
                c_lflag: 0o0000010,
                ..Termios::ZERO
            },
            signals: Vec::new(),
            open_error: None,
            tcgetattr_error: None,
            writes: Vec::new(),
            lflag_log: Vec::new(),
            closes: 0,
            tty_fd: 7,
        }
    }
}

impl TtyOps for FakeTty {
    fn open_tty(&mut self) -> Result<i32, Errno> {
        if let Some(error) = self.open_error {
            return Err(error);
        }
        Ok(self.tty_fd)
    }

    fn tcgetattr(&mut self, _fd: i32) -> Result<Termios, Errno> {
        if let Some(error) = self.tcgetattr_error {
            return Err(error);
        }
        Ok(self.termios)
    }

    fn tcsetattr(&mut self, _fd: i32, t: &Termios) -> Result<(), Errno> {
        self.lflag_log.push(t.c_lflag);
        self.termios = *t;
        Ok(())
    }

    fn write(&mut self, fd: i32, b: &[u8]) -> Result<usize, Errno> {
        self.writes.push((fd, b.to_vec()));
        Ok(b.len())
    }

    fn read_byte(&mut self, _fd: i32) -> Result<Option<u8>, Errno> {
        if !self.signals.is_empty() {
            let _ = self.signals.remove(0);
            return Err(Errno(raven_libc_abi::errno::EINTR as u16));
        }
        if self.input.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.input.remove(0)))
    }

    fn close(&mut self, _fd: i32) {
        self.closes += 1;
    }

    fn pending_signal(&mut self) -> Option<i32> {
        if self.signals.is_empty() {
            None
        } else {
            Some(self.signals.remove(0))
        }
    }
}

/// A scripted `PtyOps` with a call log; `fork` returns `Err(ENOSYS)` by default (PG-45).
#[derive(Clone, Debug)]
pub struct FakePty {
    /// Every call, in order (`"name:fd"` or `"name"`).
    pub calls: Vec<String>,
    /// Set when a step fails (name of the failing call).
    pub fail_at: Option<&'static str>,
    /// Descriptor returned by `posix_openpt`.
    pub master: i32,
    /// Descriptor returned by `open`.
    pub slave: i32,
    /// Name returned by `ptsname_r`.
    pub pts_name: Vec<u8>,
    /// The pid that `fork` reports in the parent.
    pub child_pid: i32,
    /// What `fork` does.
    pub fork_result: Result<ForkResult, Errno>,
    /// Descriptors closed, in order.
    pub closed: Vec<i32>,
}

impl Default for FakePty {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            fail_at: None,
            master: 3,
            slave: 4,
            pts_name: b"/dev/pts/1".to_vec(),
            child_pid: 99,
            fork_result: Err(Errno(raven_libc_abi::errno::ENOSYS as u16)),
            closed: Vec::new(),
        }
    }
}

impl FakePty {
    fn step(&mut self, name: &'static str) -> Result<(), Errno> {
        self.calls.push(String::from(name));
        if self.fail_at == Some(name) {
            return Err(Errno(raven_libc_abi::errno::EIO as u16));
        }
        Ok(())
    }
}

impl PtyOps for FakePty {
    fn posix_openpt(&mut self, flags: i32) -> Result<i32, Errno> {
        self.step("posix_openpt")?;
        self.calls.push(alloc::format!("flags:{flags}"));
        Ok(self.master)
    }

    fn grantpt(&mut self, _fd: i32) -> Result<(), Errno> {
        self.step("grantpt")
    }

    fn unlockpt(&mut self, _fd: i32) -> Result<(), Errno> {
        self.step("unlockpt")
    }

    fn ptsname_r(&mut self, _fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
        self.step("ptsname_r")?;
        if buf.len() < self.pts_name.len() + 1 {
            return Err(Errno(raven_libc_abi::errno::ERANGE as u16));
        }
        buf[..self.pts_name.len()].copy_from_slice(&self.pts_name);
        buf[self.pts_name.len()] = 0;
        Ok(self.pts_name.len())
    }

    fn open(&mut self, path: &[u8], _flags: i32) -> Result<i32, Errno> {
        self.step("open")?;
        self.calls.push(String::from_utf8_lossy(path).into_owned());
        Ok(self.slave)
    }

    fn close(&mut self, fd: i32) {
        self.calls.push(alloc::format!("close:{fd}"));
        self.closed.push(fd);
    }

    fn tcsetattr(&mut self, _fd: i32, _t: &Termios) -> Result<(), Errno> {
        self.step("tcsetattr")
    }

    fn set_winsize(&mut self, _fd: i32, _ws: &crate::seams::Winsize) -> Result<(), Errno> {
        self.step("set_winsize")
    }

    fn setsid(&mut self) -> Result<i32, Errno> {
        self.step("setsid")?;
        Ok(1234)
    }

    fn set_ctty(&mut self, _fd: i32) -> Result<(), Errno> {
        self.step("set_ctty")
    }

    fn dup2(&mut self, a: i32, b: i32) -> Result<i32, Errno> {
        self.step("dup2")?;
        self.calls.push(alloc::format!("dup2:{a}:{b}"));
        Ok(b)
    }

    fn fork(&mut self) -> Result<ForkResult, Errno> {
        self.calls.push(String::from("fork"));
        match self.fork_result {
            Ok(ForkResult::Parent(_)) => Ok(ForkResult::Parent(self.child_pid)),
            other => other,
        }
    }
}

/// A helper that records the bytes written to a sink.
#[derive(Clone, Debug, Default)]
pub struct Log {
    /// The recorded bytes, one entry per call.
    pub entries: RefCell<Vec<Vec<u8>>>,
}

impl Log {
    /// Record one slice.
    pub fn push(&self, bytes: &[u8]) {
        self.entries.borrow_mut().push(bytes.to_vec());
    }

    /// The concatenation of every entry.
    pub fn concat(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in self.entries.borrow().iter() {
            out.extend_from_slice(entry);
        }
        out
    }

    /// One entry as a `vec!` of bytes.
    pub fn single(&self) -> Vec<u8> {
        self.entries.borrow().first().cloned().unwrap_or_default()
    }

    /// Empty the log.
    pub fn clear(&self) {
        self.entries.borrow_mut().clear();
    }
}

/// Convenience: a `Vec<u8>` of one byte value.
pub fn byte(value: u8) -> Vec<u8> {
    vec![value]
}
