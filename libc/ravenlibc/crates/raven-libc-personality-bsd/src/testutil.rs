// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Deterministic fakes for the BSD personality seams (ticket C66 4.2).

#![allow(
    missing_docs,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::getmntinfo::StatfsLinux;
use crate::seams::{
    Errno, EvqCtl, EvqEvent, EvqOps, FdKind, FdTable, Handle, MountList, PledgeBackend, ProcCtl,
    RavenSysInfo, SrcOps, SysInfoSrc, Uname,
};

/// A fake fd table.
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

    /// Install at a chosen number.
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

    /// `FD_CLOEXEC`.
    #[must_use]
    pub fn cloexec(&self, fd: i32) -> bool {
        self.entries.get(&fd).is_some_and(|entry| entry.cloexec)
    }

    /// Number of descriptors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
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
    pub queue: Handle,
    pub events: Vec<EvqEvent>,
    pub ops: Vec<(EvqCtl, Handle, u32, u64)>,
    pub last_timeout_ns: Option<u64>,
    pub wait_error: Option<Errno>,
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
        }
    }

    /// Append an event.
    pub fn push(&mut self, cookie: u64, events: u32) {
        self.events.push(EvqEvent {
            cookie,
            events,
            pad: 0,
        });
    }

    /// The last add/modify mask for `cookie`.
    #[must_use]
    pub fn last_mask(&self, op: EvqCtl, cookie: u64) -> Option<u32> {
        self.ops
            .iter()
            .rev()
            .find(|(candidate, _, _, candidate_cookie)| {
                *candidate == op && *candidate_cookie == cookie
            })
            .map(|(_, _, mask, _)| *mask)
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

/// A fake source object factory.
#[derive(Clone, Debug)]
pub struct FakeSrc {
    pub timers: usize,
    pub events: usize,
    pub signals: Vec<(Handle, u64)>,
    pub timer_sets: Vec<(Handle, u64, u64)>,
    pub process_available: bool,
}

impl FakeSrc {
    /// A working source factory.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            timers: 0,
            events: 0,
            signals: Vec::new(),
            timer_sets: Vec::new(),
            process_available: false,
        }
    }
}

impl Default for FakeSrc {
    fn default() -> Self {
        Self::new()
    }
}

impl SrcOps for FakeSrc {
    fn timer_create(&mut self, _clock: i32) -> Result<Handle, Errno> {
        self.timers += 1;
        Ok(Handle(900u32.saturating_add(self.timers as u32)))
    }

    fn timer_set(
        &mut self,
        handle: Handle,
        initial_ns: u64,
        interval_ns: u64,
    ) -> Result<(), Errno> {
        self.timer_sets.push((handle, initial_ns, interval_ns));
        Ok(())
    }

    fn event_create(&mut self) -> Result<Handle, Errno> {
        self.events += 1;
        Ok(Handle(950u32.saturating_add(self.events as u32)))
    }

    fn event_signal(&mut self, handle: Handle, value: u64) -> Result<(), Errno> {
        self.signals.push((handle, value));
        Ok(())
    }

    fn process_handle(&mut self, pid: i32) -> Result<Handle, Errno> {
        if self.process_available {
            Ok(Handle(970u32.saturating_add(pid as u32)))
        } else {
            Err(Errno::ENOSYS)
        }
    }
}

/// A fake system-information source.
#[derive(Clone, Debug)]
pub struct FakeSysInfo {
    pub info: Option<RavenSysInfo>,
    pub uname: Uname,
    pub hostname: Vec<u8>,
    pub set_names: Vec<Vec<u8>>,
}

impl FakeSysInfo {
    /// A one-CPU source with boot time 2 s.
    #[must_use]
    pub fn new() -> Self {
        let mut uname = Uname::ZERO;
        set_field(&mut uname.sysname, b"RavenBSD");
        set_field(&mut uname.release, b"0.1");
        set_field(&mut uname.version, b"RavenBSD 0.1");
        set_field(&mut uname.machine, b"x86_64");
        set_field(&mut uname.nodename, b"raven");
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
        }
    }

    /// A source without `SYSINFO` (`PG-21`).
    #[must_use]
    pub fn without_sysinfo() -> Self {
        let mut source = Self::new();
        source.info = None;
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
    pub title_error: Option<Errno>,
}

impl FakeProc {
    /// A process with no title.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            name: [0; 16],
            title: None,
            titles: Vec::new(),
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
}

type PledgeCall = (Option<Vec<u8>>, Option<Vec<u8>>);

/// A fake pledge/unveil backend.
#[derive(Clone, Debug)]
pub struct FakePledge {
    pub applied: Vec<PledgeCall>,
    pub unveiled: Vec<PledgeCall>,
}

impl FakePledge {
    /// An accepting backend.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            applied: Vec::new(),
            unveiled: Vec::new(),
        }
    }
}

impl Default for FakePledge {
    fn default() -> Self {
        Self::new()
    }
}

impl PledgeBackend for FakePledge {
    fn apply(&mut self, promises: Option<&[u8]>, exec: Option<&[u8]>) -> Result<(), Errno> {
        self.applied
            .push((promises.map(<[u8]>::to_vec), exec.map(<[u8]>::to_vec)));
        Ok(())
    }

    fn unveil(&mut self, path: Option<&[u8]>, perms: Option<&[u8]>) -> Result<(), Errno> {
        self.unveiled
            .push((path.map(<[u8]>::to_vec), perms.map(<[u8]>::to_vec)));
        Ok(())
    }
}

/// A fake mount list.
#[derive(Clone, Debug)]
pub struct FakeMounts {
    pub mounts: Option<Vec<StatfsLinux>>,
}

impl FakeMounts {
    /// An empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self { mounts: None }
    }
}

impl Default for FakeMounts {
    fn default() -> Self {
        Self::new()
    }
}

impl MountList for FakeMounts {
    fn list(&mut self) -> Result<Vec<StatfsLinux>, Errno> {
        self.mounts.clone().ok_or(Errno::ENOSYS)
    }
}

/// Copy a text field into a C `utsname` field.
fn set_field(field: &mut [core::ffi::c_char; 65], text: &[u8]) {
    for (dst, src) in field.iter_mut().zip(text.iter()) {
        *dst = *src as core::ffi::c_char;
    }
}

/// `len` copies of `byte` (test data helper).
#[must_use]
pub fn repeated_bytes(byte: u8, len: usize) -> alloc::vec::Vec<u8> {
    alloc::vec![byte; len]
}

/// Parse one `#define NAME <integer expression>` out of an installed header.
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
        if let Some(after) = rest[name.len()..].trim_start().strip_prefix('(')
            && after.starts_with(|c: char| c.is_ascii_alphabetic())
        {
            continue;
        }
        return eval(value.trim());
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
        return parse_i64(left)?.checked_shl(u32::try_from(parse_i64(right)?).ok()?);
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
    } else {
        body.parse::<i64>().ok()?
    };
    if negative {
        value.checked_neg()
    } else {
        Some(value)
    }
}
