// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Deterministic fakes for every seam (ticket C66 4.2).
//!
//! Test-only: the module is compiled under `#[cfg(test)]` and never ships. The integration
//! tests under `tests/` carry their own minimal copies because `cfg(test)` items are not
//! visible to them.

#![allow(
    missing_docs,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use crate::seams::{
    CLOCK_MONOTONIC, CLOCK_REALTIME, Errno, EventOps, EvqCtl, EvqEvent, EvqOps, FdKind, FdTable,
    FileIo, FsWatch, Handle, ProcCtl, RavenSysInfo, SigSet, SignalOps, SysInfoSrc, TimeSrc,
    TimerOps, Uname,
};

/// A fake fd table: descriptors start at 3 and epochs increase on every install.
#[derive(Clone, Debug)]
pub struct FakeFdTable {
    entries: BTreeMap<i32, FakeEntry>,
    next_fd: i32,
    next_epoch: u32,
    pub install_error: Option<Errno>,
}

/// One fake descriptor.
#[derive(Clone, Copy, Debug)]
pub struct FakeEntry {
    pub handle: Handle,
    pub kind: FdKind,
    pub epoch: u32,
    pub cloexec: bool,
}

impl FakeFdTable {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            next_fd: 3,
            next_epoch: 1,
            install_error: None,
        }
    }

    /// Install at a chosen number (test setup).
    pub fn insert(&mut self, fd: i32, handle: Handle, kind: FdKind) {
        let epoch = self.next_epoch;
        self.next_epoch = self.next_epoch.wrapping_add(1).max(1);
        self.entries.insert(
            fd,
            FakeEntry {
                handle,
                kind,
                epoch,
                cloexec: false,
            },
        );
    }

    /// Whether `fd` is open.
    #[must_use]
    pub fn is_open(&self, fd: i32) -> bool {
        self.entries.contains_key(&fd)
    }

    /// `FD_CLOEXEC` of `fd`.
    #[must_use]
    pub fn cloexec(&self, fd: i32) -> bool {
        self.entries.get(&fd).is_some_and(|entry| entry.cloexec)
    }

    /// The number of open descriptors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no descriptor is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for FakeFdTable {
    fn default() -> Self {
        Self::new()
    }
}

impl FdTable for FakeFdTable {
    fn handle_of(&self, fd: i32) -> Result<Handle, Errno> {
        self.entries
            .get(&fd)
            .map(|entry| entry.handle)
            .ok_or(Errno::EBADF)
    }

    fn install(&mut self, handle: Handle, kind: FdKind, cloexec: bool) -> Result<i32, Errno> {
        if let Some(error) = self.install_error {
            return Err(error);
        }
        let mut fd = self.next_fd;
        while self.entries.contains_key(&fd) {
            fd = fd.saturating_add(1);
            if fd < 0 {
                return Err(Errno::EMFILE);
            }
        }
        let epoch = self.next_epoch;
        self.next_epoch = self.next_epoch.wrapping_add(1).max(1);
        self.entries.insert(
            fd,
            FakeEntry {
                handle,
                kind,
                epoch,
                cloexec,
            },
        );
        self.next_fd = fd.saturating_add(1);
        Ok(fd)
    }

    fn kind_of(&self, fd: i32) -> Result<FdKind, Errno> {
        self.entries
            .get(&fd)
            .map(|entry| entry.kind)
            .ok_or(Errno::EBADF)
    }

    fn epoch(&self, fd: i32) -> u32 {
        self.entries.get(&fd).map_or(0, |entry| entry.epoch)
    }

    fn close_range_from(&mut self, lo: i32) -> usize {
        let lo = lo.max(0);
        let keys: Vec<i32> = self
            .entries
            .keys()
            .copied()
            .filter(|fd| *fd >= lo)
            .collect();
        let count = keys.len();
        for fd in keys {
            self.entries.remove(&fd);
        }
        count
    }

    fn close_fd(&mut self, fd: i32) -> Result<(), Errno> {
        self.entries.remove(&fd).map(|_| ()).ok_or(Errno::EBADF)
    }

    fn set_cloexec(&mut self, fd: i32, on: bool) -> Result<(), Errno> {
        self.entries
            .get_mut(&fd)
            .map(|entry| entry.cloexec = on)
            .ok_or(Errno::EBADF)
    }
}

/// A scripted event queue.
#[derive(Clone, Debug)]
pub struct FakeEvq {
    /// Queue handle.
    pub queue: Handle,
    /// Pending events returned by `wait`.
    pub events: Vec<EvqEvent>,
    /// Every `ctl` call.
    pub ops: Vec<(EvqCtl, Handle, u32, u64)>,
    /// The last timeout passed to `wait`.
    pub last_timeout_ns: Option<u64>,
    /// When set, the next `wait` returns this error.
    pub wait_error: Option<Errno>,
    /// When set, `ctl` returns this error.
    pub ctl_error: Option<Errno>,
}

impl FakeEvq {
    /// A queue with no events.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            queue: Handle(100),
            events: Vec::new(),
            ops: Vec::new(),
            last_timeout_ns: None,
            wait_error: None,
            ctl_error: None,
        }
    }

    /// Append one event with the given cookie/mask.
    pub fn push(&mut self, cookie: u64, events: u32) {
        self.events.push(EvqEvent {
            cookie,
            events,
            pad: 0,
        });
    }

    /// Number of registrations currently recorded (rough model for assertions).
    #[must_use]
    pub fn registrations(&self) -> usize {
        self.ops
            .iter()
            .filter(|(op, _, _, _)| *op != EvqCtl::Del)
            .count()
    }
}

impl Default for FakeEvq {
    fn default() -> Self {
        Self::new()
    }
}

impl EvqOps for FakeEvq {
    fn create(&mut self) -> Result<Handle, Errno> {
        Ok(self.queue)
    }

    fn ctl(
        &mut self,
        q: Handle,
        op: EvqCtl,
        h: Handle,
        events: u32,
        cookie: u64,
    ) -> Result<(), Errno> {
        if q != self.queue {
            return Err(Errno::EBADF);
        }
        if let Some(error) = self.ctl_error {
            return Err(error);
        }
        self.ops.push((op, h, events, cookie));
        Ok(())
    }

    fn wait(&mut self, q: Handle, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno> {
        if q != self.queue {
            return Err(Errno::EBADF);
        }
        self.last_timeout_ns = Some(timeout_ns);
        if let Some(error) = self.wait_error {
            return Err(error);
        }
        let count = out.len().min(self.events.len());
        for (index, slot) in out.iter_mut().take(count).enumerate() {
            *slot = self.events[index];
        }
        self.events.drain(..count);
        Ok(count)
    }
}

/// A fake clock.
#[derive(Clone, Copy, Debug)]
pub struct FakeTime {
    pub now: u64,
    pub error: Option<Errno>,
}

impl FakeTime {
    /// Fixed time.
    #[must_use]
    pub const fn at(now: u64) -> Self {
        Self { now, error: None }
    }
}

impl Default for FakeTime {
    fn default() -> Self {
        Self {
            now: 1_000_000_000,
            error: None,
        }
    }
}

impl TimeSrc for FakeTime {
    fn now_ns(&self, clock: i32) -> Result<u64, Errno> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if !matches!(clock, CLOCK_REALTIME | CLOCK_MONOTONIC) {
            return Err(Errno::EINVAL);
        }
        Ok(self.now)
    }
}

/// A fake in-memory file set.
#[derive(Clone, Debug)]
pub struct FakeFiles {
    files: BTreeMap<i32, FakeFile>,
    /// Total bytes written across all files (for "error after partial" scripts).
    pub written_total: usize,
    pub read_error: Option<Errno>,
    pub write_error: Option<Errno>,
    pub write_eagain: bool,
    pub write_fail_after: Option<usize>,
}

/// One fake file.
#[derive(Clone, Debug)]
pub struct FakeFile {
    pub data: Vec<u8>,
    pub pos: usize,
    pub directory: bool,
    pub read_only: bool,
    pub seekable: bool,
}

impl FakeFiles {
    /// An empty set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            written_total: 0,
            read_error: None,
            write_error: None,
            write_eagain: false,
            write_fail_after: None,
        }
    }

    /// Insert a regular file.
    pub fn insert(&mut self, fd: i32, data: &[u8]) {
        self.files.insert(
            fd,
            FakeFile {
                data: data.to_vec(),
                pos: 0,
                directory: false,
                read_only: false,
                seekable: true,
            },
        );
    }

    /// Insert with explicit attributes.
    pub fn insert_file(&mut self, fd: i32, file: FakeFile) {
        self.files.insert(fd, file);
    }

    /// The bytes of `fd`.
    #[must_use]
    pub fn data(&self, fd: i32) -> Option<&[u8]> {
        self.files.get(&fd).map(|file| file.data.as_slice())
    }

    /// The cursor of `fd`.
    #[must_use]
    pub fn position(&self, fd: i32) -> Option<usize> {
        self.files.get(&fd).map(|file| file.pos)
    }

    fn check_read(&self, fd: i32) -> Result<&FakeFile, Errno> {
        if let Some(error) = self.read_error {
            return Err(error);
        }
        let file = self.files.get(&fd).ok_or(Errno::EBADF)?;
        if file.directory {
            return Err(Errno::EISDIR);
        }
        Ok(file)
    }
}

impl Default for FakeFiles {
    fn default() -> Self {
        Self::new()
    }
}

impl FileIo for FakeFiles {
    fn read(&mut self, fd: i32, b: &mut [u8]) -> Result<usize, Errno> {
        let position = {
            let file = self.check_read(fd)?;
            file.pos
        };
        let available = {
            let file = self.files.get(&fd).ok_or(Errno::EBADF)?;
            file.data.len().saturating_sub(position)
        };
        let count = available.min(b.len());
        if count > 0 {
            let file = self.files.get_mut(&fd).ok_or(Errno::EBADF)?;
            let Some(source) = file.data.get(position..position.saturating_add(count)) else {
                return Err(Errno::EIO);
            };
            let Some(dst) = b.get_mut(..count) else {
                return Err(Errno::EIO);
            };
            dst.copy_from_slice(source);
            file.pos = file.pos.saturating_add(count);
        }
        Ok(count)
    }

    fn write(&mut self, fd: i32, b: &[u8]) -> Result<usize, Errno> {
        if let Some(error) = self.write_error {
            return Err(error);
        }
        if self.write_eagain {
            return Err(Errno::EAGAIN);
        }
        if let Some(limit) = self.write_fail_after
            && self.written_total >= limit
        {
            return Err(Errno::EIO);
        }
        let file = self.files.get_mut(&fd).ok_or(Errno::EBADF)?;
        if file.read_only || file.directory {
            return Err(Errno::EBADF);
        }
        let position = file.pos;
        let end = position.saturating_add(b.len());
        if file.data.len() < end {
            file.data.resize(end, 0);
        }
        let Some(dst) = file.data.get_mut(position..end) else {
            return Err(Errno::EIO);
        };
        dst.copy_from_slice(b);
        file.pos = end;
        self.written_total = self.written_total.saturating_add(b.len());
        Ok(b.len())
    }

    fn pread(&mut self, fd: i32, b: &mut [u8], off: i64) -> Result<usize, Errno> {
        let file = self.check_read(fd)?;
        if !file.seekable {
            return Err(Errno::ESPIPE);
        }
        if off < 0 {
            return Err(Errno::EINVAL);
        }
        let position = usize::try_from(off).unwrap_or(usize::MAX);
        let source = file.data.get(position..).unwrap_or(&[]);
        let count = source.len().min(b.len());
        if let Some(dst) = b.get_mut(..count) {
            dst.copy_from_slice(&source[..count]);
        }
        Ok(count)
    }

    fn pwrite(&mut self, fd: i32, b: &[u8], off: i64) -> Result<usize, Errno> {
        if let Some(error) = self.write_error {
            return Err(error);
        }
        if off < 0 {
            return Err(Errno::EINVAL);
        }
        let position = usize::try_from(off).unwrap_or(usize::MAX);
        let file = self.files.get_mut(&fd).ok_or(Errno::EBADF)?;
        if file.read_only || file.directory {
            return Err(Errno::EBADF);
        }
        let end = position.saturating_add(b.len());
        if file.data.len() < end {
            file.data.resize(end, 0);
        }
        let Some(dst) = file.data.get_mut(position..end) else {
            return Err(Errno::EIO);
        };
        dst.copy_from_slice(b);
        Ok(b.len())
    }
}

/// A fake `SYSINFO`/`uname`/hostname source.
#[derive(Clone, Debug)]
pub struct FakeSysInfo {
    pub info: Option<RavenSysInfo>,
    pub uname: Uname,
    pub hostname: Vec<u8>,
    pub set_names: Vec<Vec<u8>>,
    pub info_error: Option<Errno>,
}

impl FakeSysInfo {
    /// A source with a one-CPU machine and the given boot time.
    #[must_use]
    pub fn new() -> Self {
        let mut uname = Uname::ZERO;
        set_uname_field(&mut uname.sysname, b"RavenBSD");
        set_uname_field(&mut uname.release, b"0.1");
        set_uname_field(&mut uname.version, b"RavenBSD 0.1");
        set_uname_field(&mut uname.machine, b"x86_64");
        set_uname_field(&mut uname.nodename, b"raven");
        Self {
            info: Some(RavenSysInfo {
                cpus_conf: 1,
                cpus_online: 1,
                mem_total_pages: 16384,
                mem_free_pages: 8192,
                boot_time_ns: 2_000_000_000,
            }),
            uname,
            hostname: b"raven".to_vec(),
            set_names: Vec::new(),
            info_error: None,
        }
    }

    /// A source whose `SYSINFO` is absent (`PG-21`).
    #[must_use]
    pub fn without_sysinfo() -> Self {
        let mut source = Self::new();
        source.info = None;
        source.info_error = Some(Errno::ENOSYS);
        source
    }
}

impl Default for FakeSysInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl SysInfoSrc for FakeSysInfo {
    fn sysinfo(&mut self) -> Result<RavenSysInfo, Errno> {
        if let Some(error) = self.info_error {
            return Err(error);
        }
        self.info.ok_or(Errno::ENOSYS)
    }

    fn uname(&mut self) -> Result<Uname, Errno> {
        Ok(self.uname)
    }

    fn hostname(&mut self) -> Result<Vec<u8>, Errno> {
        Ok(self.hostname.clone())
    }

    fn set_hostname(&mut self, name: &[u8]) -> Result<(), Errno> {
        if name.len() > 64 {
            return Err(Errno::EINVAL);
        }
        self.hostname = name.to_vec();
        self.set_names.push(name.to_vec());
        Ok(())
    }
}

/// A fake process record.
#[derive(Clone, Debug)]
pub struct FakeProc {
    pub name: [u8; 16],
    pub title: Option<Vec<u8>>,
    pub titles: Vec<Option<Vec<u8>>>,
    pub pdeathsig: i32,
    pub dumpable: i32,
    pub subreaper: i32,
    pub no_new_privs: i32,
    pub timerslack: u64,
    pub title_error: Option<Errno>,
}

impl FakeProc {
    /// A process with the Linux defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            name: [0; 16],
            title: None,
            titles: Vec::new(),
            pdeathsig: 0,
            dumpable: 1,
            subreaper: 0,
            no_new_privs: 0,
            timerslack: crate::prctl::TIMERSLACK_DEFAULT,
            title_error: None,
        }
    }
}

impl Default for FakeProc {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcCtl for FakeProc {
    fn prctl_name_set(&mut self, name: &[u8; 16]) -> Result<(), Errno> {
        self.name = *name;
        Ok(())
    }

    fn prctl_name_get(&mut self) -> Result<[u8; 16], Errno> {
        Ok(self.name)
    }

    fn set_title(&mut self, title: Option<&[u8]>) -> Result<(), Errno> {
        if let Some(error) = self.title_error {
            return Err(error);
        }
        let value = title.map(|text| text.to_vec());
        self.title = value.clone();
        self.titles.push(value);
        Ok(())
    }

    fn prctl_pdeathsig_set(&mut self, signal: i32) -> Result<(), Errno> {
        self.pdeathsig = signal;
        Ok(())
    }

    fn prctl_pdeathsig_get(&mut self) -> Result<i32, Errno> {
        Ok(self.pdeathsig)
    }

    fn prctl_dumpable_set(&mut self, value: i32) -> Result<(), Errno> {
        self.dumpable = value;
        Ok(())
    }

    fn prctl_dumpable_get(&mut self) -> Result<i32, Errno> {
        Ok(self.dumpable)
    }

    fn prctl_subreaper_set(&mut self, value: i32) -> Result<(), Errno> {
        self.subreaper = value;
        Ok(())
    }

    fn prctl_subreaper_get(&mut self) -> Result<i32, Errno> {
        Ok(self.subreaper)
    }

    fn prctl_no_new_privs_set(&mut self) -> Result<(), Errno> {
        self.no_new_privs = 1;
        Ok(())
    }

    fn prctl_no_new_privs_get(&mut self) -> Result<i32, Errno> {
        Ok(self.no_new_privs)
    }

    fn prctl_timerslack_set(&mut self, nanoseconds: u64) -> Result<(), Errno> {
        self.timerslack = nanoseconds;
        Ok(())
    }

    fn prctl_timerslack_get(&mut self) -> Result<u64, Errno> {
        Ok(self.timerslack)
    }
}

/// A fake user-event source.
#[derive(Clone, Debug)]
pub struct FakeEvent {
    pub created: usize,
    pub signals: Vec<Handle>,
    pub counter: u64,
    pub unavailable: bool,
}

impl FakeEvent {
    /// A working source.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            created: 0,
            signals: Vec::new(),
            counter: 0,
            unavailable: false,
        }
    }
}

impl Default for FakeEvent {
    fn default() -> Self {
        Self::new()
    }
}

impl EventOps for FakeEvent {
    fn create(&mut self) -> Result<Handle, Errno> {
        if self.unavailable {
            return Err(Errno::ENOSYS);
        }
        self.created += 1;
        Ok(Handle(200u32.saturating_add(self.created as u32)))
    }

    fn signal(&mut self, handle: Handle) -> Result<(), Errno> {
        self.signals.push(handle);
        self.counter = self.counter.saturating_add(1);
        Ok(())
    }

    fn read_counter(&mut self, _handle: Handle) -> Result<u64, Errno> {
        if self.unavailable {
            return Err(Errno::ENOSYS);
        }
        Ok(self.counter)
    }

    fn wait_readable(&mut self, _handle: Handle) -> Result<(), Errno> {
        if self.unavailable {
            return Err(Errno::ENOSYS);
        }
        if self.counter == 0 {
            return Err(Errno::EAGAIN);
        }
        Ok(())
    }
}

/// A fake timer source.
#[derive(Clone, Debug)]
pub struct FakeTimer {
    pub created: Vec<i32>,
    pub sets: Vec<(Handle, u64, u64)>,
    pub expirations: u64,
    pub unavailable: bool,
}

impl FakeTimer {
    /// A working source.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            created: Vec::new(),
            sets: Vec::new(),
            expirations: 0,
            unavailable: false,
        }
    }
}

impl Default for FakeTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerOps for FakeTimer {
    fn create(&mut self, clock: i32) -> Result<Handle, Errno> {
        if self.unavailable {
            return Err(Errno::ENOSYS);
        }
        self.created.push(clock);
        Ok(Handle(300u32.saturating_add(self.created.len() as u32)))
    }

    fn set(&mut self, handle: Handle, initial_ns: u64, interval_ns: u64) -> Result<(), Errno> {
        if self.unavailable {
            return Err(Errno::ENOSYS);
        }
        self.sets.push((handle, initial_ns, interval_ns));
        Ok(())
    }

    fn get(&mut self, _handle: Handle) -> Result<(u64, u64), Errno> {
        if self.unavailable {
            return Err(Errno::ENOSYS);
        }
        self.sets
            .last()
            .map(|(_, initial, interval)| (*initial, *interval))
            .ok_or(Errno::ENOENT)
    }

    fn read_expirations(&mut self, _handle: Handle) -> Result<u64, Errno> {
        if self.unavailable {
            return Err(Errno::ENOSYS);
        }
        Ok(self.expirations)
    }
}

/// A fake signal source; the default is the documented `PG-7` absence.
#[derive(Clone, Debug)]
pub struct FakeSignal {
    pub available: bool,
    pub masks: Vec<SigSet>,
}

impl FakeSignal {
    /// An unavailable source.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            available: false,
            masks: Vec::new(),
        }
    }
}

impl Default for FakeSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalOps for FakeSignal {
    fn create(&mut self, mask: &SigSet, _flags: i32) -> Result<Handle, Errno> {
        if !self.available {
            return Err(Errno::ENOSYS);
        }
        self.masks.push(*mask);
        Ok(Handle(400))
    }

    fn update(&mut self, _handle: Handle, mask: &SigSet) -> Result<(), Errno> {
        if !self.available {
            return Err(Errno::ENOSYS);
        }
        self.masks.push(*mask);
        Ok(())
    }
}

/// A fake filesystem watch service; the default is the documented `PG-38` absence.
#[derive(Clone, Debug)]
pub struct FakeWatch {
    pub available: bool,
    pub watches: Vec<(i32, Vec<u8>, u32)>,
}

impl FakeWatch {
    /// An unavailable service.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            available: false,
            watches: Vec::new(),
        }
    }
}

impl Default for FakeWatch {
    fn default() -> Self {
        Self::new()
    }
}

impl FsWatch for FakeWatch {
    fn init(&mut self, _flags: i32) -> Result<Handle, Errno> {
        if self.available {
            Ok(Handle(500))
        } else {
            Err(Errno::ENOSYS)
        }
    }

    fn add_watch(&mut self, fd: i32, path: &[u8], mask: u32) -> Result<i32, Errno> {
        if !self.available {
            return Err(Errno::ENOSYS);
        }
        self.watches.push((fd, path.to_vec(), mask));
        Ok(i32::try_from(self.watches.len()).unwrap_or(1))
    }

    fn rm_watch(&mut self, _fd: i32, _wd: i32) -> Result<(), Errno> {
        if self.available {
            Ok(())
        } else {
            Err(Errno::ENOSYS)
        }
    }
}

/// Convenience: a one-event mask.
#[must_use]
pub fn event(cookie: u64, events: u32) -> EvqEvent {
    EvqEvent {
        cookie,
        events,
        pad: 0,
    }
}

/// A vector with the given bytes repeated (test data helper).
#[must_use]
pub fn repeated(byte: u8, len: usize) -> Vec<u8> {
    vec![byte; len]
}

/// Copy a text field into a C `utsname` field.
fn set_uname_field(field: &mut [core::ffi::c_char; 65], text: &[u8]) {
    for (dst, src) in field.iter_mut().zip(text.iter()) {
        *dst = *src as core::ffi::c_char;
    }
}

/// Parse one `#define NAME <integer expression>` out of an installed header.
///
/// The expressions in the headers this ticket checks are integer literals, `U`/`L` suffixes,
/// parentheses, unary minus, `<<` and `|`; anything else returns `None` and the calling test
/// fails loudly instead of silently accepting a re-typed number.
#[must_use]
pub fn header_define(text: &str, name: &str) -> Option<i64> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("#define") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(value) = rest.strip_prefix(name) else {
            continue;
        };
        if !value.starts_with(char::is_whitespace) && !value.starts_with('(') {
            continue;
        }
        let expression = value.trim();
        // Reject function-like macros: `NAME(` immediately followed by an identifier.
        if let Some(after) = rest[name.len()..].trim_start().strip_prefix('(')
            && after.starts_with(|c: char| c.is_ascii_alphabetic())
        {
            continue;
        }
        return eval(expression);
    }
    None
}

fn eval(expression: &str) -> Option<i64> {
    let text: String = expression
        .chars()
        .filter(|c| {
            !c.is_whitespace()
                && *c != '('
                && *c != ')'
                && *c != 'u'
                && *c != 'U'
                && *c != 'l'
                && *c != 'L'
        })
        .collect();
    if let Some((left, right)) = text.split_once("<<") {
        let base = parse_i64(left)?;
        let shift = u32::try_from(parse_i64(right)?).ok()?;
        return base.checked_shl(shift);
    }
    if let Some((left, right)) = text.split_once('|') {
        return Some(parse_i64(left)? | parse_i64(right)?);
    }
    parse_i64(&text)
}

fn parse_i64(text: &str) -> Option<i64> {
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let value = if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()?
    } else if let Some(octal) = body.strip_prefix('0')
        && !octal.is_empty()
        && octal.chars().all(|c| c.is_ascii_digit() && c < '8')
    {
        i64::from_str_radix(octal, 8).ok()?
    } else {
        body.parse::<i64>().ok()?
    };
    if negative {
        value.checked_neg()
    } else {
        Some(value)
    }
}
