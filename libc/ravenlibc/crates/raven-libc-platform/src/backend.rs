// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The kernel and scheme seam.
//!
//! [`SysBackend`] is the only way this crate reaches the kernel. [`RealSys`] forwards to the safe
//! typed wrappers of `raven-libc-sys`; [`FakeSys`] is a deterministic in-memory world used by the
//! unit tests and by `tests/support/fake_sys.rs`. Nothing above this module knows whether a
//! primitive is a syscall or a test double.
//!
//! Calls that the kernel cannot serve yet are named by [`PendingCall`] and answered with
//! `ENOSYS`; the caller registers the class-A stub from [`crate::stubs`] with its gap id.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_abi::uname::Uname;
use raven_libc_abi::structs::Timespec;
use raven_libc_sys::sys::EvqEvent;

use crate::fd::OfdKind;
use crate::sysinfo::SysInfo;

/// A primitive the kernel does not implement yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingCall {
    /// Positional read (`PG-22`).
    Pread,
    /// Positional write (`PG-22`).
    Pwrite,
    /// `lseek` (`PG-22`).
    Seek,
    /// One-shot `POLL` (`PG-26`).
    PollOnce,
    /// Kernel entropy (`PG-52`).
    Getrandom,
    /// System information (`PG-21`).
    Sysinfo,
    /// `MADVISE` (`PG-15`).
    Madvise,
    /// File mappings (`PG-36`).
    FileMap,
}

/// The operations the platform layer needs from below.
pub trait SysBackend {
    /// Open a scheme path with wire open flags.
    fn open(&self, path: &[u8], wire_flags: u32) -> Result<Handle, Errno>;
    /// Read from a handle.
    fn read(&self, handle: Handle, buf: &mut [u8]) -> Result<usize, Errno>;
    /// Write to a handle.
    fn write(&self, handle: Handle, buf: &[u8]) -> Result<usize, Errno>;
    /// Close a handle.
    fn close(&self, handle: Handle) -> Result<(), Errno>;
    /// Duplicate a handle with a rights mask.
    fn dup(&self, handle: Handle, rights: u32) -> Result<Handle, Errno>;
    /// Fill the 64-byte `FSTAT` record.
    fn fstat(&self, handle: Handle, out: &mut [u8; 64]) -> Result<(), Errno>;
    /// Send a `CALL` request and return the response length.
    fn call(&self, handle: Handle, req: &[u8], resp: &mut [u8]) -> Result<usize, Errno>;
    /// Transfer a handle to another process.
    fn handle_transfer(&self, dest: Handle, src: Handle, rights: Rights) -> Result<Handle, Errno>;
    /// Create an event queue.
    fn evq_create(&self) -> Result<Handle, Errno>;
    /// Add, modify or delete an event-queue registration.
    fn evq_ctl(
        &self,
        queue: Handle,
        op: u32,
        handle: Handle,
        events: u32,
        cookie: u64,
    ) -> Result<(), Errno>;
    /// Wait for events.
    fn evq_wait(
        &self,
        queue: Handle,
        out: &mut [EvqEvent],
        timeout_ns: u64,
    ) -> Result<usize, Errno>;
    /// Map memory and return the address.
    fn map(
        &self,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        prot: u32,
        flags: u32,
    ) -> Result<usize, Errno>;
    /// Unmap memory.
    fn unmap(&self, addr: usize, len: usize) -> Result<(), Errno>;
    /// Change protection.
    fn protect(&self, addr: usize, len: usize, prot: u32) -> Result<(), Errno>;
    /// Create a memory object.
    fn memobj_create(&self, len: usize, flags: u32) -> Result<Handle, Errno>;
    /// Read a clock.
    fn clock_get(&self, id: u32) -> Result<Timespec, Errno>;
    /// Sleep for a duration.
    fn nanosleep(&self, ns: u64) -> Result<(), Errno>;
    /// Read the system identification record.
    fn uname(&self) -> Result<Uname, Errno> {
        raven_libc_sys::sys::uname()
    }
    /// Read the system-information counters (PG-21); `ENOSYS` by default.
    fn sysinfo(&self) -> Result<SysInfo, Errno> {
        Err(Errno::ENOSYS)
    }

    /// Diagnostic output; the shell routes it to `DEBUG_WRITE`.
    fn debug_write(&self, bytes: &[u8]) -> Result<usize, Errno> {
        raven_libc_sys::sys::debug_write(bytes)
    }

    /// Terminate the process after an unrecoverable fdsan violation.
    fn exit_abort(&self, code: i32) -> ! {
        raven_libc_sys::sys::exit(code)
    }

    /// A primitive that is not implemented yet; `ENOSYS` by default.
    fn pending(&self, _call: PendingCall) -> Result<isize, Errno> {
        Err(Errno::ENOSYS)
    }
}

/// The real backend: typed wrappers over the Raven system calls.
#[derive(Clone, Copy, Debug, Default)]
pub struct RealSys;

impl SysBackend for RealSys {
    fn open(&self, path: &[u8], wire_flags: u32) -> Result<Handle, Errno> {
        raven_libc_sys::sys::open(path, wire_flags)
    }

    fn read(&self, handle: Handle, buf: &mut [u8]) -> Result<usize, Errno> {
        raven_libc_sys::sys::read(handle, buf)
    }

    fn write(&self, handle: Handle, buf: &[u8]) -> Result<usize, Errno> {
        raven_libc_sys::sys::write(handle, buf)
    }

    fn close(&self, handle: Handle) -> Result<(), Errno> {
        raven_libc_sys::sys::close(handle)
    }

    fn dup(&self, handle: Handle, rights: u32) -> Result<Handle, Errno> {
        raven_libc_sys::sys::dup(handle, rights)
    }

    fn fstat(&self, handle: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
        raven_libc_sys::sys::fstat(handle, out)
    }

    fn call(&self, handle: Handle, req: &[u8], resp: &mut [u8]) -> Result<usize, Errno> {
        raven_libc_sys::sys::call(handle, req, resp)
    }

    fn handle_transfer(&self, dest: Handle, src: Handle, rights: Rights) -> Result<Handle, Errno> {
        raven_libc_sys::sys::handle_transfer(dest, src, rights)
    }

    fn evq_create(&self) -> Result<Handle, Errno> {
        raven_libc_sys::sys::evq_create(0)
    }

    fn evq_ctl(
        &self,
        queue: Handle,
        op: u32,
        handle: Handle,
        events: u32,
        cookie: u64,
    ) -> Result<(), Errno> {
        raven_libc_sys::sys::evq_ctl(queue, op, handle, events, cookie)
    }

    fn evq_wait(
        &self,
        queue: Handle,
        out: &mut [EvqEvent],
        timeout_ns: u64,
    ) -> Result<usize, Errno> {
        raven_libc_sys::sys::evq_wait(queue, out, timeout_ns)
    }

    fn map(
        &self,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        prot: u32,
        flags: u32,
    ) -> Result<usize, Errno> {
        raven_libc_sys::sys::map(Handle::SELF_PROC, memobj, offset, addr, len, prot, flags)
    }

    fn unmap(&self, addr: usize, len: usize) -> Result<(), Errno> {
        raven_libc_sys::sys::unmap(Handle::SELF_PROC, addr, len)
    }

    fn protect(&self, addr: usize, len: usize, prot: u32) -> Result<(), Errno> {
        raven_libc_sys::sys::protect(Handle::SELF_PROC, addr, len, prot)
    }

    fn memobj_create(&self, len: usize, flags: u32) -> Result<Handle, Errno> {
        raven_libc_sys::sys::memobj_create(len, flags as usize)
    }

    fn clock_get(&self, id: u32) -> Result<Timespec, Errno> {
        let time = raven_libc_sys::sys::clock_get(id)?;
        Ok(Timespec {
            tv_sec: time.tv_sec,
            tv_nsec: time.tv_nsec,
        })
    }

    fn nanosleep(&self, ns: u64) -> Result<(), Errno> {
        raven_libc_sys::sys::nanosleep(ns)
    }
}

/// A fault injected into [`FakeSys`] by a test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The next `open`.
    Open,
    /// The next `read`.
    Read,
    /// The next `write`.
    Write,
    /// The next `close`.
    Close,
    /// The next `fstat`.
    Fstat,
    /// The next `call`.
    Call,
    /// The next `map`.
    Map,
    /// The next `evq_wait`.
    EvqWait,
    /// The next `nanosleep`.
    Nanosleep,
}

#[derive(Clone, Debug)]
struct FakeNode {
    data: Vec<u8>,
    mode: u32,
    kind: OfdKind,
    target: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
struct FakeOpen {
    path: Vec<u8>,
    cursor: usize,
    writable: bool,
    kind: OfdKind,
}

#[derive(Clone, Debug, Default)]
struct FakeEvent {
    handle: u32,
    events: u32,
    cookie: u64,
}

struct FakeWorld {
    nodes: BTreeMap<Vec<u8>, FakeNode>,
    handles: BTreeMap<u32, u32>,
    open_files: BTreeMap<u32, FakeOpen>,
    tty_settings: BTreeMap<Vec<u8>, raven_proto_tty::Settings>,
    next_handle: u32,
    next_file: u32,
    objects: BTreeMap<u32, Vec<u8>>,
    maps: BTreeMap<usize, (usize, u32, Vec<u8>)>,
    next_map: usize,
    queues: BTreeMap<u32, Vec<FakeEvent>>,
    ready: BTreeMap<u32, u32>,
    next_queue: u32,
    faults: Vec<(Fault, Errno)>,
    clock_ns: u64,
    sleeps: Vec<u64>,
    debug_output: Vec<u8>,
    pub trace: Vec<u64>,
}

/// Deterministic in-memory backend for host tests.
///
/// The fake models one scheme namespace: `file:` nodes created by [`FakeSys::file`], `tty:` nodes
/// created by [`FakeSys::tty`], `sys:` values created by [`FakeSys::sys`] and `rand:` bytes. It
/// implements the operation precedence of the real kernel: every method first inspects
/// [`FakeSys::fail_next`] so a test can force one error without touching the rest of the world.
pub struct FakeSys {
    world: RefCell<FakeWorld>,
}

impl Default for FakeSys {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeSys {
    /// An empty world: no files, no handles, a deterministic clock at zero.
    pub fn new() -> Self {
        Self {
            world: RefCell::new(FakeWorld {
                nodes: BTreeMap::new(),
                handles: BTreeMap::new(),
                open_files: BTreeMap::new(),
                tty_settings: BTreeMap::new(),
                next_handle: 5,
                next_file: 100,
                objects: BTreeMap::new(),
                maps: BTreeMap::new(),
                next_map: 0x1000_0000,
                queues: BTreeMap::new(),
                ready: BTreeMap::new(),
                next_queue: 0x8000_0000,
                faults: Vec::new(),
                clock_ns: 0,
                sleeps: Vec::new(),
                debug_output: Vec::new(),
                trace: Vec::new(),
            }),
        }
    }

    /// Create or replace a regular file at an absolute POSIX path.
    pub fn file(&self, path: &[u8], data: &[u8]) -> &Self {
        self.node(path, data, 0o100_644, OfdKind::File);
        self
    }

    /// Create a directory at an absolute POSIX path.
    pub fn dir(&self, path: &[u8]) -> &Self {
        self.node(path, &[], 0o040_755, OfdKind::Dir);
        self
    }

    /// Create a symbolic link at an absolute POSIX path.
    pub fn symlink(&self, path: &[u8], target: &[u8]) -> &Self {
        self.node(path, &[], 0o120_777, OfdKind::File);
        let mut world = self.world.borrow_mut();
        if let Some(node) = world.nodes.get_mut(&key(path)) {
            node.target = Some(target.to_vec());
        }
        drop(world);
        self
    }

    fn node(&self, path: &[u8], data: &[u8], mode: u32, kind: OfdKind) {
        self.world.borrow_mut().nodes.insert(
            key(path),
            FakeNode {
                data: data.to_vec(),
                mode,
                kind,
                target: None,
            },
        );
    }

    /// Create a tty node.
    pub fn tty(&self, path: &[u8]) -> &Self {
        self.node(path, &[], 0o020_620, OfdKind::Tty);
        self.world
            .borrow_mut()
            .tty_settings
            .insert(key(path), raven_proto_tty::Settings::default());
        self
    }

    /// Create a `sys:` value addressed as `sys:a/b`.
    pub fn sys(&self, path: &[u8], value: &[u8]) -> &Self {
        let mut full = b"sys:".to_vec();
        full.extend_from_slice(path);
        self.world.borrow_mut().nodes.insert(
            full,
            FakeNode {
                data: value.to_vec(),
                mode: 0o100_644,
                kind: OfdKind::File,
                target: None,
            },
        );
        self
    }

    /// Create a `rand:` source with deterministic bytes.
    pub fn rand(&self, bytes: &[u8]) -> &Self {
        self.world.borrow_mut().nodes.insert(
            b"rand:".to_vec(),
            FakeNode {
                data: bytes.to_vec(),
                mode: 0o100_644,
                kind: OfdKind::File,
                target: None,
            },
        );
        self
    }

    /// The stored contents of a file, if present.
    pub fn contents(&self, path: &[u8]) -> Option<Vec<u8>> {
        self.world
            .borrow()
            .nodes
            .get(&key(path))
            .map(|node| node.data.clone())
    }

    /// Whether an absolute path exists in the fake tree.
    pub fn exists(&self, path: &[u8]) -> bool {
        self.world.borrow().nodes.contains_key(&key(path))
    }

    /// Force one operation to fail with `errno`.
    pub fn fail_next(&self, fault: Fault, errno: Errno) {
        self.world.borrow_mut().faults.push((fault, errno));
    }

    /// Advance the virtual monotonic clock.
    pub fn advance_ns(&self, ns: u64) {
        let mut world = self.world.borrow_mut();
        world.clock_ns = world.clock_ns.saturating_add(ns);
    }

    /// Monotonic nanoseconds since the fake started.
    pub fn now_ns(&self) -> u64 {
        self.world.borrow().clock_ns
    }

    /// Durations passed to `nanosleep`, in call order.
    pub fn sleeps(&self) -> Vec<u64> {
        self.world.borrow().sleeps.clone()
    }

    /// Make an event handle readable with `events` until the next wait.
    pub fn ready(&self, handle: Handle, events: u32) {
        self.world.borrow_mut().ready.insert(handle.0, events);
    }

    /// Number of open kernel handles (for leak checks).
    pub fn open_handles(&self) -> usize {
        self.world.borrow().handles.len()
    }

    /// Bytes written through `DEBUG_WRITE`.
    pub fn debug_output(&self) -> Vec<u8> {
        self.world.borrow().debug_output.clone()
    }

    /// The bytes of a mapping, if any.
    pub fn mapped(&self, addr: usize) -> Option<Vec<u8>> {
        self.world
            .borrow()
            .maps
            .get(&addr)
            .map(|(_, _, data)| data.clone())
    }

    /// Write into a mapping.
    pub fn poke(&self, addr: usize, offset: usize, data: &[u8]) -> bool {
        let mut world = self.world.borrow_mut();
        match world.maps.get_mut(&addr) {
            Some((_, _, bytes)) => {
                let end = match offset.checked_add(data.len()) {
                    Some(end) => end,
                    None => return false,
                };
                match bytes.get_mut(offset..end) {
                    Some(slice) => {
                        slice.copy_from_slice(data);
                        true
                    }
                    None => false,
                }
            }
            None => false,
        }
    }

    fn take_fault(&self, fault: Fault) -> Result<(), Errno> {
        let mut world = self.world.borrow_mut();
        if let Some(index) = world
            .faults
            .iter()
            .position(|(candidate, _)| *candidate == fault)
        {
            let (_, errno) = world.faults.remove(index);
            return Err(errno);
        }
        Ok(())
    }

    fn record(&self, op: u64) {
        self.world.borrow_mut().trace.push(op);
    }
}

fn key(path: &[u8]) -> Vec<u8> {
    let mut full = b"file:".to_vec();
    full.extend_from_slice(path);
    full
}

fn open_kind(wire_flags: u32) -> (bool, bool) {
    (
        wire_flags & raven_proto_fs::WIRE_OPEN_READ as u32 != 0,
        wire_flags & raven_proto_fs::WIRE_OPEN_WRITE as u32 != 0,
    )
}

impl SysBackend for FakeSys {
    fn open(&self, path: &[u8], wire_flags: u32) -> Result<Handle, Errno> {
        self.take_fault(Fault::Open)?;
        self.record(0x4F50_454E);
        let mut world = self.world.borrow_mut();
        let create = wire_flags & u32::from(raven_proto_fs::WIRE_OPEN_CREATE) != 0;
        let exclusive = wire_flags & u32::from(raven_proto_fs::WIRE_OPEN_EXCL) != 0;
        let truncate = wire_flags & u32::from(raven_proto_fs::WIRE_OPEN_TRUNC) != 0;
        let existing = world.nodes.contains_key(path);
        if existing && create && exclusive {
            return Err(Errno::EEXIST);
        }
        let raw = match world.nodes.get(path).cloned() {
            Some(node) => node,
            None if create => {
                let node = FakeNode {
                    data: Vec::new(),
                    mode: 0o100_666,
                    kind: OfdKind::File,
                    target: None,
                };
                world.nodes.insert(path.to_vec(), node.clone());
                node
            }
            None => return Err(Errno::ENOENT),
        };
        if raw.kind == OfdKind::Dir
            && wire_flags & u32::from(raven_proto_fs::WIRE_OPEN_CREATE) != 0
            && wire_flags & u32::from(raven_proto_fs::WIRE_OPEN_DIRECTORY) == 0
        {
            return Err(Errno::EISDIR);
        }
        if raw.kind != OfdKind::Dir
            && wire_flags & u32::from(raven_proto_fs::WIRE_OPEN_DIRECTORY) != 0
        {
            return Err(Errno::ENOTDIR);
        }
        let (readable, writable) = open_kind(wire_flags);
        if !readable && !writable {
            return Err(Errno::EINVAL);
        }
        if truncate
            && writable
            && let Some(node) = world.nodes.get_mut(path)
        {
            node.data.clear();
        }
        let handle = world.next_handle;
        world.next_handle = world.next_handle.saturating_add(1);
        let file = world.next_file;
        world.next_file = world.next_file.saturating_add(1);
        world.open_files.insert(
            file,
            FakeOpen {
                path: path.to_vec(),
                cursor: 0,
                writable,
                kind: raw.kind,
            },
        );
        world.handles.insert(handle, file);
        Ok(Handle(handle))
    }

    fn read(&self, handle: Handle, buf: &mut [u8]) -> Result<usize, Errno> {
        self.take_fault(Fault::Read)?;
        self.record(0x5245_4144);
        let mut world = self.world.borrow_mut();
        let file = *world.handles.get(&handle.0).ok_or(Errno::EBADF)?;
        let open = world.open_files.get(&file).cloned().ok_or(Errno::EBADF)?;
        if open.kind == OfdKind::Dir {
            return Err(Errno::EISDIR);
        }
        let node = world.nodes.get(&open.path).cloned().ok_or(Errno::ENOENT)?;
        let start = open.cursor.min(node.data.len());
        let available = node.data.get(start..).unwrap_or(&[]);
        let count = available.len().min(buf.len());
        buf.get_mut(..count)
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(available.get(..count).unwrap_or(&[]));
        if let Some(entry) = world.open_files.get_mut(&file) {
            entry.cursor = start.saturating_add(count);
        }
        Ok(count)
    }

    fn write(&self, handle: Handle, buf: &[u8]) -> Result<usize, Errno> {
        self.take_fault(Fault::Write)?;
        self.record(0x5752_4954);
        let mut world = self.world.borrow_mut();
        let file = *world.handles.get(&handle.0).ok_or(Errno::EBADF)?;
        let open = world.open_files.get(&file).cloned().ok_or(Errno::EBADF)?;
        if !open.writable {
            return Err(Errno::EBADF);
        }
        let node = world.nodes.get_mut(&open.path).ok_or(Errno::ENOENT)?;
        let end = open.cursor.saturating_add(buf.len());
        if end > node.data.len() {
            node.data.resize(end, 0);
        }
        match node.data.get_mut(open.cursor..end) {
            Some(slice) => slice.copy_from_slice(buf),
            None => return Err(Errno::EIO),
        }
        if let Some(entry) = world.open_files.get_mut(&file) {
            entry.cursor = end;
        }
        Ok(buf.len())
    }

    fn close(&self, handle: Handle) -> Result<(), Errno> {
        self.take_fault(Fault::Close)?;
        self.record(0x434C_4F53);
        let mut world = self.world.borrow_mut();
        let file = world.handles.remove(&handle.0).ok_or(Errno::EBADF)?;
        if !world.handles.values().any(|candidate| *candidate == file) {
            world.open_files.remove(&file);
        }
        Ok(())
    }

    fn dup(&self, handle: Handle, _rights: u32) -> Result<Handle, Errno> {
        self.record(0x4455_5020);
        let mut world = self.world.borrow_mut();
        let file = *world.handles.get(&handle.0).ok_or(Errno::EBADF)?;
        let new = world.next_handle;
        world.next_handle = world.next_handle.saturating_add(1);
        world.handles.insert(new, file);
        Ok(Handle(new))
    }

    fn fstat(&self, handle: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
        self.take_fault(Fault::Fstat)?;
        self.record(0x4653_5441);
        let world = self.world.borrow();
        let file = *world.handles.get(&handle.0).ok_or(Errno::EBADF)?;
        let open = world.open_files.get(&file).ok_or(Errno::EBADF)?;
        let node = world.nodes.get(&open.path).ok_or(Errno::ENOENT)?;
        let record = raven_proto_fs::Stat {
            ino: u64::from(handle.0) + 100,
            size: node.data.len() as u64,
            blocks: node.data.len().div_ceil(512) as u64,
            mtime: 1_700_000_000,
            ctime: 1_700_000_000,
            atime: 1_700_000_000,
            mode: node.mode,
            nlink: if node.kind == OfdKind::Dir { 2 } else { 1 },
            uid: 1000,
            gid: 1000,
        };
        record.encode(out).map_err(|error| error.errno())
    }

    fn call(&self, handle: Handle, req: &[u8], resp: &mut [u8]) -> Result<usize, Errno> {
        self.take_fault(Fault::Call)?;
        self.record(0x4341_4C4C);
        let mut world = self.world.borrow_mut();
        let file = *world.handles.get(&handle.0).ok_or(Errno::EBADF)?;
        let open = world.open_files.get(&file).cloned().ok_or(Errno::EBADF)?;
        if open.kind == OfdKind::Tty
            && req.len() == raven_proto_tty::CALL_LEN
            && raven_proto_tty::decode_request(req).is_ok()
        {
            let mut body = [0u8; raven_proto_tty::CALL_LEN];
            let request = raven_proto_tty::decode_request(req).map_err(|_| Errno::EPROTO)?;
            let settings = *world.tty_settings.entry(open.path.clone()).or_default();
            let response = match request {
                raven_proto_tty::Request::GetMode => raven_proto_tty::Response::Mode(settings),
                raven_proto_tty::Request::SetMode { settings: new } => {
                    world.tty_settings.insert(open.path.clone(), new);
                    raven_proto_tty::Response::Mode(new)
                }
                raven_proto_tty::Request::Flush => raven_proto_tty::Response::Flushed { bytes: 0 },
            };
            raven_proto_tty::encode_response(response, &mut body);
            let copied = body.len().min(resp.len());
            resp.get_mut(..copied)
                .ok_or(Errno::EINVAL)?
                .copy_from_slice(body.get(..copied).unwrap_or(&[]));
            return Ok(copied);
        }
        let request = raven_proto_fs::CallRequest::decode(req).map_err(|error| error.errno())?;
        let response = match request.call {
            raven_proto_fs::Call::Statfs => raven_proto_fs::CallResponse::StatFs {
                status: 0,
                statfs: raven_proto_fs::StatFs {
                    block_size: 4096,
                    blocks: 1024,
                    free_blocks: 512,
                    files: 64,
                    free_files: 32,
                },
            },
            raven_proto_fs::Call::Mkdir { mode, path } => {
                let full = key(path);
                world.nodes.insert(
                    full,
                    FakeNode {
                        data: Vec::new(),
                        mode: raven_libc_abi::stat::S_IFDIR | (mode & 0o7777),
                        kind: OfdKind::Dir,
                        target: None,
                    },
                );
                raven_proto_fs::CallResponse::Status(0)
            }
            raven_proto_fs::Call::Rmdir { path } => {
                world.nodes.remove(&key(path));
                raven_proto_fs::CallResponse::Status(0)
            }
            raven_proto_fs::Call::Unlink { path } => {
                world.nodes.remove(&key(path));
                raven_proto_fs::CallResponse::Status(0)
            }
            raven_proto_fs::Call::Rename { from, to } => {
                if let Some(node) = world.nodes.remove(&key(from)) {
                    world.nodes.insert(key(to), node);
                }
                raven_proto_fs::CallResponse::Status(0)
            }
            raven_proto_fs::Call::Chmod { mode, path } => {
                if let Some(node) = world.nodes.get_mut(&key(path)) {
                    node.mode = (node.mode & raven_libc_abi::stat::S_IFMT) | (mode & 0o7777);
                }
                raven_proto_fs::CallResponse::Status(0)
            }
            raven_proto_fs::Call::Symlink { path, target } => {
                world.nodes.insert(
                    key(path),
                    FakeNode {
                        data: Vec::new(),
                        mode: raven_libc_abi::stat::S_IFLNK | 0o777,
                        kind: OfdKind::File,
                        target: Some(target.to_vec()),
                    },
                );
                raven_proto_fs::CallResponse::Status(0)
            }
            raven_proto_fs::Call::Readlink { path } => {
                match world
                    .nodes
                    .get(&key(path))
                    .and_then(|node| node.target.clone())
                {
                    Some(target) => {
                        let mut response = alloc::vec![0u8; 4 + target.len()];
                        let encoded = raven_proto_fs::CallResponse::Readlink {
                            status: 0,
                            target: &target,
                        }
                        .encode(&mut response)
                        .map_err(|error| error.errno())?;
                        let copied = encoded.min(resp.len());
                        resp.get_mut(..copied)
                            .ok_or(Errno::EINVAL)?
                            .copy_from_slice(response.get(..copied).unwrap_or(&[]));
                        return Ok(copied);
                    }
                    None => raven_proto_fs::CallResponse::Status(-(Errno::EINVAL.0 as i32)),
                }
            }
            raven_proto_fs::Call::Truncate { .. }
            | raven_proto_fs::Call::Sync
            | raven_proto_fs::Call::Mount { .. }
            | raven_proto_fs::Call::Umount { .. } => raven_proto_fs::CallResponse::Status(0),
        };
        let _ = open;
        response.encode(resp).map_err(|error| error.errno())
    }

    fn handle_transfer(&self, _dest: Handle, src: Handle, rights: Rights) -> Result<Handle, Errno> {
        self.dup(src, rights.bits())
    }

    fn evq_create(&self) -> Result<Handle, Errno> {
        self.record(0x4556_5143);
        let mut world = self.world.borrow_mut();
        let queue = world.next_queue;
        world.next_queue = world.next_queue.saturating_add(1);
        world.queues.insert(queue, Vec::new());
        Ok(Handle(queue))
    }

    fn evq_ctl(
        &self,
        queue: Handle,
        op: u32,
        handle: Handle,
        events: u32,
        cookie: u64,
    ) -> Result<(), Errno> {
        self.record(0x4556_5143_0001);
        let mut world = self.world.borrow_mut();
        let registrations = world.queues.get_mut(&queue.0).ok_or(Errno::EBADF)?;
        const ADD: u32 = 1;
        const MOD: u32 = 2;
        const DEL: u32 = 3;
        match op {
            ADD => registrations.push(FakeEvent {
                handle: handle.0,
                events,
                cookie,
            }),
            DEL => registrations.retain(|entry| entry.handle != handle.0),
            MOD => {
                for entry in registrations.iter_mut() {
                    if entry.handle == handle.0 {
                        entry.events = events;
                        entry.cookie = cookie;
                    }
                }
            }
            _ => return Err(Errno::EINVAL),
        }
        Ok(())
    }

    fn evq_wait(
        &self,
        queue: Handle,
        out: &mut [EvqEvent],
        timeout_ns: u64,
    ) -> Result<usize, Errno> {
        self.take_fault(Fault::EvqWait)?;
        self.record(0x4556_5157);
        let mut world = self.world.borrow_mut();
        if timeout_ns != 0 {
            world.clock_ns = world.clock_ns.saturating_add(timeout_ns);
        }
        let registrations = world.queues.get(&queue.0).cloned().ok_or(Errno::EBADF)?;
        let mut written = 0usize;
        for entry in registrations {
            if written >= out.len() {
                break;
            }
            if let Some(mask) = world.ready.remove(&entry.handle) {
                let delivered = mask & entry.events;
                if delivered == 0 {
                    continue;
                }
                if let Some(slot) = out.get_mut(written) {
                    *slot = EvqEvent {
                        cookie: entry.cookie,
                        events: delivered,
                        pad: 0,
                    };
                    written = written.saturating_add(1);
                }
            }
        }
        Ok(written)
    }

    fn map(
        &self,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        prot: u32,
        flags: u32,
    ) -> Result<usize, Errno> {
        self.take_fault(Fault::Map)?;
        self.record(0x4D41_5020);
        if len == 0 || offset != 0 {
            return Err(Errno::EINVAL);
        }
        let mut world = self.world.borrow_mut();
        let base = if memobj == Handle::NONE {
            vec![0u8; len]
        } else {
            let object = world.objects.get(&memobj.0).cloned().ok_or(Errno::EBADF)?;
            let mut bytes = vec![0u8; len];
            let copy = object.len().min(len);
            bytes
                .get_mut(..copy)
                .ok_or(Errno::EIO)?
                .copy_from_slice(object.get(..copy).unwrap_or(&[]));
            bytes
        };
        let address = if flags & raven_abi::types::map::FIXED as u32 != 0 || addr != 0 {
            addr
        } else {
            world.next_map
        };
        if world.maps.contains_key(&address) {
            return Err(Errno::ENOMEM);
        }
        world.maps.insert(address, (len, prot, base));
        world.next_map = address.saturating_add(len).saturating_add(0x1000);
        Ok(address)
    }

    fn unmap(&self, addr: usize, len: usize) -> Result<(), Errno> {
        self.record(0x4D55_4E4D);
        let mut world = self.world.borrow_mut();
        let (mapped_len, _, _) = world.maps.get(&addr).cloned().ok_or(Errno::EINVAL)?;
        if len != mapped_len {
            return Err(Errno::EINVAL);
        }
        world.maps.remove(&addr);
        Ok(())
    }

    fn protect(&self, addr: usize, len: usize, prot: u32) -> Result<(), Errno> {
        self.record(0x4D50_524F);
        let mut world = self.world.borrow_mut();
        match world.maps.get_mut(&addr) {
            Some((mapped_len, current, _)) if *mapped_len == len => {
                *current = prot;
                Ok(())
            }
            _ => Err(Errno::EINVAL),
        }
    }

    fn memobj_create(&self, len: usize, _flags: u32) -> Result<Handle, Errno> {
        self.record(0x4D45_4D4F);
        let mut world = self.world.borrow_mut();
        let handle = world.next_handle;
        world.next_handle = world.next_handle.saturating_add(1);
        world.objects.insert(handle, vec![0u8; len]);
        Ok(Handle(handle))
    }

    fn clock_get(&self, id: u32) -> Result<Timespec, Errno> {
        self.record(0x434C_4F43);
        let world = self.world.borrow();
        let ns = world.clock_ns;
        match id {
            0 | 1 => Ok(Timespec {
                tv_sec: (ns / 1_000_000_000) as i64 + 1_700_000_000,
                tv_nsec: (ns % 1_000_000_000) as i64,
            }),
            _ => Err(Errno::EINVAL),
        }
    }

    fn uname(&self) -> Result<Uname, Errno> {
        let mut record = Uname::zeroed();
        Uname::set_field(&mut record.sysname, "RavenBSD");
        Uname::set_field(&mut record.nodename, "fake");
        Uname::set_field(&mut record.release, "0.1");
        Uname::set_field(&mut record.version, "host-fake");
        Uname::set_field(&mut record.machine, "x86_64");
        Ok(record)
    }

    fn sysinfo(&self) -> Result<SysInfo, Errno> {
        Err(Errno::ENOSYS)
    }

    fn nanosleep(&self, ns: u64) -> Result<(), Errno> {
        self.take_fault(Fault::Nanosleep)?;
        self.record(0x4E41_4E4F);
        let mut world = self.world.borrow_mut();
        world.sleeps.push(ns);
        world.clock_ns = world.clock_ns.saturating_add(ns);
        Ok(())
    }

    fn debug_write(&self, bytes: &[u8]) -> Result<usize, Errno> {
        let mut world = self.world.borrow_mut();
        world.debug_output.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn exit_abort(&self, code: i32) -> ! {
        raven_libc_sys::sys::exit(code)
    }

    fn pending(&self, call: PendingCall) -> Result<isize, Errno> {
        let _ = call;
        Err(Errno::ENOSYS)
    }
}

/// Boxed error-free helper used by tests that need to build a path without a `Vec` import.
pub fn boxed(bytes: &[u8]) -> Box<[u8]> {
    bytes.to_vec().into_boxed_slice()
}

#[cfg(test)]
#[path = "backend_ut.rs"]
mod tests;
