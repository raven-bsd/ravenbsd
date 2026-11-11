// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K66 filesystem-contract fixture: a fake server and client in one process.
//!
//! The client encodes real S11 `SchemePacket`s, the server is the real
//! `raven-scheme::Server` dispatcher, and the filesystem below it is an in-memory
//! model.  The contract itself is `docs/design/fs-contract.md`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    raven_rt::sys::exit(101)
}

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
use raven_proto_fs::{
    DirEntry, DirEntryHeader, MAX_PATH, OpenFlags, S_IFDIR, S_IFLNK, S_IFMT, S_IFREG, Stat,
    WIRE_OPEN_DIRECTORY, dirent_kind, normalize_request, split_first_component,
};
use raven_scheme::{
    Ctx, Handler, HandlerResult, Kernel, OpenTable, Opened, Packet, Server, SpinLock,
};

/// Root inode.
pub const ROOT: u64 = 1;

struct Node {
    mode: u32,
    nlink: u32,
    generation: u32,
    kind: Kind,
}

enum Kind {
    File(Vec<u8>),
    Dir(BTreeMap<String, u64>),
    Symlink(String),
}

struct OpenState {
    ino: u64,
    dir_generation: Option<u32>,
}

/// In-memory filesystem implementing the K66 contract.
pub struct FakeFs {
    nodes: BTreeMap<u64, Node>,
    next_ino: u64,
    opens: OpenTable<OpenState>,
}

impl FakeFs {
    /// A filesystem with a root directory.
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            ROOT,
            Node {
                mode: S_IFDIR | 0o755,
                nlink: 2,
                generation: 1,
                kind: Kind::Dir(BTreeMap::new()),
            },
        );
        Self {
            nodes,
            next_ino: ROOT + 1,
            opens: OpenTable::new(),
        }
    }

    fn alloc(&mut self, mode: u32, kind: Kind) -> u64 {
        let ino = self.next_ino;
        self.next_ino += 1;
        let nlink = if matches!(kind, Kind::Dir(_)) { 2 } else { 1 };
        self.nodes.insert(
            ino,
            Node {
                mode,
                nlink,
                generation: 1,
                kind,
            },
        );
        ino
    }

    fn node(&self, ino: u64) -> Result<&Node, Errno> {
        self.nodes.get(&ino).ok_or(Errno::ENOENT)
    }

    fn dir(&self, ino: u64) -> Result<&BTreeMap<String, u64>, Errno> {
        match &self.node(ino)?.kind {
            Kind::Dir(children) => Ok(children),
            _ => Err(Errno::ENOTDIR),
        }
    }

    fn bump(&mut self, ino: u64) {
        if let Some(node) = self.nodes.get_mut(&ino) {
            node.generation = node.generation.wrapping_add(1).max(1);
        }
    }

    fn insert_child(&mut self, parent: u64, name: &str, ino: u64) -> Result<(), Errno> {
        let Node {
            kind: Kind::Dir(children),
            ..
        } = self.nodes.get_mut(&parent).ok_or(Errno::ENOENT)?
        else {
            return Err(Errno::ENOTDIR);
        };
        if children.contains_key(name) {
            return Err(Errno::EEXIST);
        }
        children.insert(name.to_string(), ino);
        self.bump(parent);
        Ok(())
    }

    /// Create a regular file with an explicit mode.
    pub fn create_file(&mut self, parent: u64, name: &str, mode: u32) -> Result<u64, Errno> {
        let ino = self.alloc(S_IFREG | (mode & 0o7777), Kind::File(Vec::new()));
        self.insert_child(parent, name, ino)?;
        Ok(ino)
    }

    /// Create a directory with an explicit mode.
    pub fn create_dir(&mut self, parent: u64, name: &str, mode: u32) -> Result<u64, Errno> {
        let ino = self.alloc(S_IFDIR | (mode & 0o7777), Kind::Dir(BTreeMap::new()));
        self.insert_child(parent, name, ino)?;
        Ok(ino)
    }

    /// Create a symbolic link.
    pub fn create_symlink(&mut self, parent: u64, name: &str, target: &str) -> Result<u64, Errno> {
        let ino = self.alloc(S_IFLNK | 0o777, Kind::Symlink(target.to_string()));
        self.insert_child(parent, name, ino)?;
        Ok(ino)
    }

    /// Write bytes directly, for test setup.
    pub fn write_file(&mut self, ino: u64, data: &[u8]) -> Result<(), Errno> {
        match &mut self.nodes.get_mut(&ino).ok_or(Errno::ENOENT)?.kind {
            Kind::File(file) => {
                file.clear();
                file.extend_from_slice(data);
                Ok(())
            }
            _ => Err(Errno::EISDIR),
        }
    }

    /// Change stored mode bits.
    pub fn chmod(&mut self, ino: u64, mode: u32) -> Result<(), Errno> {
        let node = self.nodes.get_mut(&ino).ok_or(Errno::ENOENT)?;
        node.mode = (node.mode & S_IFMT) | (mode & 0o7777);
        Ok(())
    }

    /// Remove a directory entry.
    pub fn unlink(&mut self, parent: u64, name: &str) -> Result<(), Errno> {
        let Node {
            kind: Kind::Dir(children),
            ..
        } = self.nodes.get_mut(&parent).ok_or(Errno::ENOENT)?
        else {
            return Err(Errno::ENOTDIR);
        };
        if children.remove(name).is_some() {
            self.bump(parent);
            Ok(())
        } else {
            Err(Errno::ENOENT)
        }
    }

    /// Look up a path, following symbolic links.
    ///
    /// The server normally receives a relative path; a leading `/` is accepted
    /// here for test setup convenience.
    pub fn lookup(&self, path: &str) -> Result<u64, Errno> {
        self.resolve_from(ROOT, path.trim_start_matches('/'), true)
    }

    fn resolve_from(&self, start: u64, path: &str, follow_final: bool) -> Result<u64, Errno> {
        let mut current = start;
        let mut rest = path.to_string();
        let mut depth = 0u8;
        while !rest.is_empty() {
            let (component, remainder) = split_first_component(&rest).map_err(|error| {
                if error == raven_proto_fs::PathError::DotComponent {
                    Errno::EXDEV
                } else {
                    error.errno()
                }
            })?;
            let component = component.to_string();
            let remainder = remainder.to_string();
            let children = self.dir(current)?;
            let child = *children.get(&component).ok_or(Errno::ENOENT)?;
            let link = match &self.node(child)?.kind {
                Kind::Symlink(target) => Some(target.clone()),
                _ => None,
            };
            if let Some(target) = link {
                if remainder.is_empty() && !follow_final {
                    return Ok(child);
                }
                depth = depth.saturating_add(1);
                if depth > 8 {
                    return Err(Errno::ELOOP);
                }
                if let Some(absolute) = target.strip_prefix('/') {
                    current = ROOT;
                    rest = absolute.to_string();
                } else {
                    rest = if remainder.is_empty() {
                        target
                    } else {
                        alloc::format!("{target}/{remainder}")
                    };
                }
            } else {
                current = child;
                rest = remainder;
            }
        }
        Ok(current)
    }

    fn parent_of(&self, path: &str) -> Result<(u64, String), Errno> {
        match path.rsplit_once('/') {
            Some((parent, name)) if !name.is_empty() => {
                Ok((self.resolve_from(ROOT, parent, true)?, name.to_string()))
            }
            _ => Ok((ROOT, path.to_string())),
        }
    }

    fn open_file(&mut self, path: &str, flags: OpenFlags) -> Result<u64, Errno> {
        if flags.directory() && flags.create() {
            return Err(Errno::EINVAL);
        }
        let existing = if path.is_empty() {
            Some(ROOT)
        } else {
            match self.resolve_from(ROOT, path, !flags.nofollow()) {
                Ok(ino) => Some(ino),
                Err(Errno::ENOENT) => None,
                Err(error) => return Err(error),
            }
        };
        let ino = match existing {
            Some(ino) => {
                if flags.create() && flags.exclusive() {
                    return Err(Errno::EEXIST);
                }
                if flags.create() && matches!(self.node(ino)?.kind, Kind::Dir(_)) {
                    return Err(Errno::EISDIR);
                }
                if flags.nofollow() && matches!(self.node(ino)?.kind, Kind::Symlink(_)) {
                    return Err(Errno::ELOOP);
                }
                ino
            }
            None if flags.create() => {
                let (parent, name) = self.parent_of(path)?;
                let mode = flags.create_mode().unwrap_or(0o644);
                self.create_file(parent, &name, mode)?
            }
            None => return Err(Errno::ENOENT),
        };
        let is_dir = matches!(self.node(ino)?.kind, Kind::Dir(_));
        if flags.directory() && !is_dir {
            return Err(Errno::ENOTDIR);
        }
        if is_dir && flags.truncate() {
            return Err(Errno::EISDIR);
        }
        if is_dir && flags.writable() {
            return Err(Errno::EISDIR);
        }
        if flags.truncate()
            && let Kind::File(data) = &mut self.nodes.get_mut(&ino).ok_or(Errno::ENOENT)?.kind
        {
            data.clear();
        }
        self.opens.insert(
            OpenState {
                ino,
                dir_generation: None,
            },
            flags,
        )
    }

    fn read_into(&mut self, id: u64, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        if offset != 0 {
            return Err(Errno::EINVAL);
        }
        let (ino, flags, cursor, generation) = {
            let entry = self.opens.get(id).ok_or(Errno::EBADF)?;
            (
                entry.file.ino,
                entry.flags,
                entry.cursor(),
                entry.file.dir_generation,
            )
        };
        if !flags.readable() {
            return Err(Errno::EBADF);
        }
        let node = self.node(ino)?;
        let (written, next_cursor, next_generation) = match &node.kind {
            Kind::File(data) => {
                let start = usize::try_from(cursor).map_err(|_| Errno::EOVERFLOW)?;
                let available = data.get(start..).unwrap_or(&[]);
                let length = available.len().min(buffer.len());
                buffer[..length].copy_from_slice(&available[..length]);
                (length, cursor + length as u64, generation)
            }
            Kind::Dir(children) => {
                let current_generation = node.generation;
                let generation = match generation {
                    Some(recorded) if recorded != current_generation => return Err(Errno::EINVAL),
                    Some(recorded) => recorded,
                    None => current_generation,
                };
                let start = usize::try_from(cursor).map_err(|_| Errno::EOVERFLOW)?;
                let mut written = 0usize;
                let mut index = 0usize;
                for (name, child_ino) in children.iter() {
                    if index < start {
                        index += 1;
                        continue;
                    }
                    let child = self.node(*child_ino)?;
                    let kind = dirent_kind(child.mode);
                    let header = DirEntryHeader {
                        ino: *child_ino,
                        kind,
                        name_len: u8::try_from(name.len()).map_err(|_| Errno::ENAMETOOLONG)?,
                        pad: [0; 6],
                    };
                    let entry = DirEntry {
                        header,
                        name: name.as_bytes(),
                    };
                    let length = entry.encoded_len();
                    if length > buffer.len() - written {
                        if written == 0 {
                            return Err(Errno::EINVAL);
                        }
                        break;
                    }
                    entry
                        .encode(&mut buffer[written..])
                        .map_err(|_| Errno::EINVAL)?;
                    written += length;
                    index += 1;
                }
                (written, index as u64, Some(generation))
            }
            Kind::Symlink(_) => return Err(Errno::ELOOP),
        };
        let entry = self.opens.get_mut(id).ok_or(Errno::EBADF)?;
        entry.set_cursor(next_cursor);
        entry.file.dir_generation = next_generation;
        Ok(written)
    }

    fn write_from(&mut self, id: u64, offset: u64, data: &[u8]) -> Result<usize, Errno> {
        if offset != 0 {
            return Err(Errno::EINVAL);
        }
        let (ino, flags, cursor) = {
            let entry = self.opens.get(id).ok_or(Errno::EBADF)?;
            (entry.file.ino, entry.flags, entry.cursor())
        };
        if !flags.writable() {
            return Err(Errno::EBADF);
        }
        let append = flags.append();
        let file = match &mut self.nodes.get_mut(&ino).ok_or(Errno::ENOENT)?.kind {
            Kind::File(data) => data,
            Kind::Dir(_) => return Err(Errno::EISDIR),
            Kind::Symlink(_) => return Err(Errno::ELOOP),
        };
        let start = if append {
            file.len()
        } else {
            usize::try_from(cursor).map_err(|_| Errno::EOVERFLOW)?
        };
        if start > file.len() {
            return Err(Errno::EINVAL);
        }
        let end = start.checked_add(data.len()).ok_or(Errno::EOVERFLOW)?;
        if end > file.len() {
            file.resize(end, 0);
        }
        file[start..end].copy_from_slice(data);
        let entry = self.opens.get_mut(id).ok_or(Errno::EBADF)?;
        entry.set_cursor(end as u64);
        Ok(data.len())
    }

    fn stat_of(&self, id: u64) -> Result<Stat, Errno> {
        let entry = self.opens.get(id).ok_or(Errno::EBADF)?;
        let node = self.node(entry.file.ino)?;
        let size = match &node.kind {
            Kind::File(data) => data.len() as u64,
            Kind::Dir(_) => 0,
            Kind::Symlink(target) => target.len() as u64,
        };
        Ok(Stat {
            ino: entry.file.ino,
            size,
            blocks: size.div_ceil(512),
            mtime: 0,
            ctime: 0,
            atime: 0,
            mode: node.mode,
            nlink: node.nlink,
            uid: 0,
            gid: 0,
        })
    }
}

impl Default for FakeFs {
    fn default() -> Self {
        Self::new()
    }
}

impl Handler for FakeFs {
    fn open_with(
        &mut self,
        path: &str,
        flags: OpenFlags,
        _mode: u32,
        _ctx: &Ctx,
    ) -> HandlerResult<Opened> {
        let id = self.open_file(path, flags)?;
        Ok(Opened::File(id))
    }

    fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        Ok(self.read_into(file, offset, buffer)?)
    }

    fn write(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
        Ok(self.write_from(file, offset, buffer)?)
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        self.opens.close(file)?;
        Ok(())
    }

    fn dup(&mut self, file: u64) -> HandlerResult<u64> {
        Ok(self.opens.dup(file)?)
    }

    fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
        Ok(self.stat_of(file)?)
    }
}

#[derive(Clone)]
struct RecordingKernel {
    sent: Arc<SpinLock<Vec<Vec<u8>>>>,
}

impl Default for RecordingKernel {
    fn default() -> Self {
        Self {
            sent: Arc::new(SpinLock::new(Vec::new())),
        }
    }
}

impl RecordingKernel {
    fn take(&self) -> Option<Vec<u8>> {
        self.sent.lock().pop()
    }
}

impl Kernel for RecordingKernel {
    fn scheme_register(&self, _name: &str) -> Result<(), Errno> {
        Ok(())
    }

    fn send(&self, _request: Handle, packet: &[u8]) -> Result<(), Errno> {
        self.sent.lock().push(packet.to_vec());
        Ok(())
    }
}

/// The fake client plus the real server dispatcher and the in-memory filesystem.
pub struct Harness {
    fs: FakeFs,
    server: Server<RecordingKernel>,
    kernel: RecordingKernel,
    next_request: u64,
}

impl Harness {
    /// Create the fixture.
    pub fn new() -> Self {
        let kernel = RecordingKernel::default();
        let server = Server::register_with(kernel.clone(), "k66").expect("register");
        Self {
            fs: FakeFs::new(),
            server,
            kernel,
            next_request: 1,
        }
    }

    /// Borrow the model for setup.
    pub fn fs(&mut self) -> &mut FakeFs {
        &mut self.fs
    }

    fn dispatch(&mut self, packet: &SchemePacket) -> Result<SchemePacket, Errno> {
        let mut bytes = [0u8; PACKET_SIZE];
        protocol::encode(packet, &mut bytes).map_err(|_| Errno::EPROTO)?;
        let request = protocol::decode(&bytes).map_err(|_| Errno::EPROTO)?;
        let handle = Handle(self.next_request as u32);
        self.next_request += 1;
        self.server
            .dispatch(
                Packet {
                    handle,
                    packet: request,
                },
                &mut self.fs,
            )
            .map_err(|_| Errno::EIO)?;
        let response = self.kernel.take().ok_or(Errno::EIO)?;
        let response = protocol::decode(&response).map_err(|_| Errno::EPROTO)?;
        if response.status != 0 {
            return Err(Errno(
                u16::try_from(response.status).map_err(|_| Errno::EIO)?,
            ));
        }
        Ok(response)
    }

    fn relative(&self, path: &str) -> Result<(String, bool), Errno> {
        let mut buffer = [0u8; MAX_PATH];
        let normalized = normalize_request(path, &mut buffer).map_err(|error| error.errno())?;
        let relative = normalized.path.strip_prefix('/').unwrap_or(normalized.path);
        Ok((relative.to_string(), normalized.requires_dir))
    }

    /// Open an absolute path with validated flags.
    pub fn open(&mut self, path: &str, flags: OpenFlags) -> Result<u64, Errno> {
        let (relative, requires_dir) = self.relative(path)?;
        let flags = if requires_dir {
            OpenFlags::from_wire(flags.to_wire() | WIRE_OPEN_DIRECTORY)
                .map_err(|error| error.errno())?
        } else {
            flags
        };
        let mut packet = SchemePacket::request(Opcode::Open, self.next_request, 0);
        packet.flags = flags.to_wire();
        packet
            .set_path(relative.as_bytes())
            .map_err(|_| Errno::ENAMETOOLONG)?;
        let response = self.dispatch(&packet)?;
        Ok(response.server_cookie)
    }

    /// Read from the open-file-description cursor.
    pub fn read(&mut self, file: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        self.read_at(file, 0, buffer)
    }

    /// Read with an explicit wire offset (used to prove nonzero is rejected).
    pub fn read_at(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        let mut packet = SchemePacket::request(Opcode::Read, self.next_request, file);
        packet.offset = offset;
        packet.buffer_addr = buffer.as_mut_ptr() as u64;
        packet.buffer_len = buffer.len() as u64;
        let response = self.dispatch(&packet)?;
        usize::try_from(response.buffer_len).map_err(|_| Errno::EOVERFLOW)
    }

    /// Write at the open-file-description cursor.
    pub fn write(&mut self, file: u64, data: &[u8]) -> Result<usize, Errno> {
        self.write_at(file, 0, data)
    }

    /// Write with an explicit wire offset (used to prove nonzero is rejected).
    pub fn write_at(&mut self, file: u64, offset: u64, data: &[u8]) -> Result<usize, Errno> {
        let mut packet = SchemePacket::request(Opcode::Write, self.next_request, file);
        packet.offset = offset;
        packet.buffer_addr = data.as_ptr() as u64;
        packet.buffer_len = data.len() as u64;
        let response = self.dispatch(&packet)?;
        usize::try_from(response.buffer_len).map_err(|_| Errno::EOVERFLOW)
    }

    /// Duplicate a server file id.
    pub fn dup(&mut self, file: u64) -> Result<u64, Errno> {
        let packet = SchemePacket::request(Opcode::Dup, self.next_request, file);
        let response = self.dispatch(&packet)?;
        Ok(response.server_cookie)
    }

    /// Close a server file id.
    pub fn close(&mut self, file: u64) -> Result<(), Errno> {
        let packet = SchemePacket::request(Opcode::Close, self.next_request, file);
        self.dispatch(&packet).map(|_| ())
    }

    /// Stat a server file id.
    pub fn fstat(&mut self, file: u64) -> Result<Stat, Errno> {
        let packet = SchemePacket::request(Opcode::Fstat, self.next_request, file);
        let response = self.dispatch(&packet)?;
        Stat::decode(response.data_bytes()).map_err(|_| Errno::EPROTO)
    }

    /// Number of open-file descriptions the server holds.
    pub fn open_count(&self) -> usize {
        self.fs.opens.len()
    }
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

fn expect(condition: bool, message: &'static str) -> Result<(), &'static str> {
    if condition { Ok(()) } else { Err(message) }
}

fn expect_err(
    result: Result<impl core::fmt::Debug, Errno>,
    error: Errno,
) -> Result<(), &'static str> {
    match result {
        Err(actual) if actual == error => Ok(()),
        _ => Err("unexpected error"),
    }
}

/// Run every contract check; the binary prints the final line on success.
pub fn run_contract() -> Result<(), &'static str> {
    flags_contract()?;
    offset_contract()?;
    dup_contract()?;
    path_contract()?;
    readdir_contract()?;
    permission_contract()?;
    Ok(())
}

fn flags_contract() -> Result<(), &'static str> {
    let mut harness = Harness::new();
    let read = OpenFlags::from_linux(raven_proto_fs::O_RDONLY).map_err(|_| "read flags")?;
    let write = OpenFlags::from_linux(raven_proto_fs::O_WRONLY).map_err(|_| "write flags")?;
    expect(
        read.to_linux() == raven_proto_fs::O_RDONLY,
        "read roundtrip",
    )?;
    expect(
        write.to_linux() == raven_proto_fs::O_WRONLY,
        "write roundtrip",
    )?;
    expect(
        OpenFlags::from_linux(0x8000_0000).is_err(),
        "unknown Linux flags accepted",
    )?;
    expect(
        OpenFlags::from_wire(0x0100).is_err(),
        "unknown wire flags accepted",
    )?;

    let create_excl = OpenFlags::from_linux(
        raven_proto_fs::O_RDWR | raven_proto_fs::O_CREAT | raven_proto_fs::O_EXCL,
    )
    .map_err(|_| "excl flags")?;
    let exclusive = harness
        .open("/exclusive", create_excl)
        .map_err(|_| "exclusive create")?;
    expect_err(harness.open("/exclusive", create_excl), Errno::EEXIST)?;
    let stat = harness.fstat(exclusive).map_err(|_| "stat exclusive")?;
    expect(stat.mode & 0o777 == 0o644, "create mode is not 0644")?;

    let create = OpenFlags::from_linux(
        raven_proto_fs::O_RDWR | raven_proto_fs::O_CREAT | raven_proto_fs::O_APPEND,
    )
    .map_err(|_| "append flags")?;
    let first = harness
        .open("/append", create)
        .map_err(|_| "append open 1")?;
    let second = harness
        .open("/append", create)
        .map_err(|_| "append open 2")?;
    harness.write(first, b"one").map_err(|_| "append one")?;
    harness.write(second, b"two").map_err(|_| "append two")?;
    let read_back = OpenFlags::from_linux(raven_proto_fs::O_RDONLY).map_err(|_| "read")?;
    let reader = harness
        .open("/append", read_back)
        .map_err(|_| "read open")?;
    let mut buffer = [0u8; 16];
    let length = harness
        .read(reader, &mut buffer)
        .map_err(|_| "append read")?;
    expect(&buffer[..length] == b"onetwo", "append order")?;

    let trunc = OpenFlags::from_linux(raven_proto_fs::O_WRONLY | raven_proto_fs::O_TRUNC)
        .map_err(|_| "trunc flags")?;
    harness.open("/append", trunc).map_err(|_| "trunc open")?;
    let stat = harness.fstat(reader).map_err(|_| "stat after trunc")?;
    // `reader` still has an OFD; read its stat size after the truncate.
    expect(stat.size == 0, "trunc did not clear the file")?;

    let directory = OpenFlags::from_linux(raven_proto_fs::O_RDONLY | raven_proto_fs::O_DIRECTORY)
        .map_err(|_| "directory flags")?;
    expect_err(harness.open("/append", directory), Errno::ENOTDIR)?;

    let root = harness.fs().lookup("/").map_err(|_| "root lookup")?;
    harness
        .fs()
        .create_symlink(root, "link", "append")
        .map_err(|_| "symlink")?;
    let nofollow = OpenFlags::from_linux(raven_proto_fs::O_RDONLY | raven_proto_fs::O_NOFOLLOW)
        .map_err(|_| "nofollow flags")?;
    expect_err(harness.open("/link", nofollow), Errno::ELOOP)?;
    harness
        .open(
            "/link",
            OpenFlags::from_linux(raven_proto_fs::O_RDONLY).map_err(|_| "follow")?,
        )
        .map_err(|_| "follow symlink")?;
    Ok(())
}

fn offset_contract() -> Result<(), &'static str> {
    let mut harness = Harness::new();
    let root = harness.fs().lookup("/").map_err(|_| "root")?;
    let ino = harness
        .fs()
        .create_file(root, "data", 0o644)
        .map_err(|_| "create data")?;
    harness
        .fs()
        .write_file(ino, b"hello world")
        .map_err(|_| "write data")?;

    let flags = OpenFlags::from_linux(raven_proto_fs::O_RDWR).map_err(|_| "flags")?;
    let file = harness.open("/data", flags).map_err(|_| "open data")?;
    let mut first = [0u8; 5];
    let mut second = [0u8; 6];
    expect(
        harness.read(file, &mut first).map_err(|_| "read 1")? == 5,
        "short read",
    )?;
    expect(
        harness.read(file, &mut second).map_err(|_| "read 2")? == 6,
        "short read 2",
    )?;
    expect(&first == b"hello", "first chunk")?;
    expect(&second == b" world", "second chunk")?;
    expect(
        harness.read(file, &mut first).map_err(|_| "eof")? == 0,
        "eof",
    )?;

    expect_err(harness.read_at(file, 1, &mut first), Errno::EINVAL)?;
    expect_err(harness.write_at(file, 1, b"x"), Errno::EINVAL)?;
    Ok(())
}

fn dup_contract() -> Result<(), &'static str> {
    let mut harness = Harness::new();
    let root = harness.fs().lookup("/").map_err(|_| "root")?;
    let ino = harness
        .fs()
        .create_file(root, "dup", 0o644)
        .map_err(|_| "create")?;
    harness
        .fs()
        .write_file(ino, b"abcdefghij")
        .map_err(|_| "write")?;
    let flags = OpenFlags::from_linux(raven_proto_fs::O_RDONLY).map_err(|_| "flags")?;
    let original = harness.open("/dup", flags).map_err(|_| "open")?;
    let mut first = [0u8; 4];
    expect(
        harness.read(original, &mut first).map_err(|_| "read")? == 4,
        "read prefix",
    )?;
    let duplicate = harness.dup(original).map_err(|_| "dup")?;
    expect(duplicate == original, "dup returned a new file id")?;
    harness.close(original).map_err(|_| "close original")?;
    expect(harness.open_count() == 1, "first close released the OFD")?;
    let mut rest = [0u8; 6];
    expect(
        harness
            .read(duplicate, &mut rest)
            .map_err(|_| "survivor read")?
            == 6,
        "survivor read length",
    )?;
    expect(&rest == b"efghij", "survivor lost the cursor")?;
    harness.close(duplicate).map_err(|_| "close duplicate")?;
    expect(harness.open_count() == 0, "last close kept the OFD")?;
    expect_err(harness.read(duplicate, &mut rest), Errno::EBADF)?;
    Ok(())
}

fn path_contract() -> Result<(), &'static str> {
    let mut harness = Harness::new();
    {
        let root = harness.fs().lookup("/").map_err(|_| "root")?;
        let ino = harness
            .fs()
            .create_file(root, "file", 0o644)
            .map_err(|_| "file")?;
        harness.fs().write_file(ino, b"x").map_err(|_| "write")?;
        let dir = harness
            .fs()
            .create_dir(root, "dir", 0o755)
            .map_err(|_| "dir")?;
        harness
            .fs()
            .create_file(dir, "inner", 0o644)
            .map_err(|_| "inner")?;
        harness
            .fs()
            .create_symlink(root, "link", "file")
            .map_err(|_| "file link")?;
        harness
            .fs()
            .create_symlink(root, "dirlink", "dir")
            .map_err(|_| "dir link")?;
    }

    let mut buffer = [0u8; MAX_PATH];
    let cases: &[(&str, &str, bool)] = &[
        ("/", "/", true),
        ("//", "/", true),
        ("/.", "/", true),
        ("/dir/..", "/", true),
        ("/dir//", "/dir", true),
        ("/dir/.", "/dir", true),
        ("/dir/inner", "/dir/inner", false),
    ];
    for (input, expected, requires_dir) in cases {
        let normalized = normalize_request(input, &mut buffer).map_err(|_| "normalize")?;
        expect(normalized.path == *expected, "normalized path")?;
        expect(
            normalized.requires_dir == *requires_dir,
            "directory requirement",
        )?;
    }

    let read = OpenFlags::from_linux(raven_proto_fs::O_RDONLY).map_err(|_| "read")?;
    harness.open("/", read).map_err(|_| "root open")?;
    harness.open("//", read).map_err(|_| "double slash")?;
    harness.open("/.", read).map_err(|_| "dot root")?;
    harness.open("/dir/..", read).map_err(|_| "dotdot")?;
    harness.open("/dir/", read).map_err(|_| "directory slash")?;
    expect_err(harness.open("/file/", read), Errno::ENOTDIR)?;
    harness.open("/link", read).map_err(|_| "symlink follow")?;
    harness.open("/dirlink/", read).map_err(|_| "dir symlink")?;
    expect_err(harness.open("/missing", read), Errno::ENOENT)?;
    let nofollow = OpenFlags::from_linux(raven_proto_fs::O_RDONLY | raven_proto_fs::O_NOFOLLOW)
        .map_err(|_| "nofollow")?;
    expect_err(harness.open("/link", nofollow), Errno::ELOOP)?;
    Ok(())
}

fn readdir_contract() -> Result<(), &'static str> {
    let mut harness = Harness::new();
    let dir = {
        let root = harness.fs().lookup("/").map_err(|_| "root")?;
        harness
            .fs()
            .create_dir(root, "list", 0o755)
            .map_err(|_| "dir")?
    };
    for name in ["a", "b", "c"] {
        harness
            .fs()
            .create_file(dir, name, 0o644)
            .map_err(|_| "entry")?;
    }
    let flags = OpenFlags::from_linux(raven_proto_fs::O_RDONLY | raven_proto_fs::O_DIRECTORY)
        .map_err(|_| "flags")?;
    let file = harness.open("/list", flags).map_err(|_| "open dir")?;
    let mut tiny = [0u8; 8];
    expect_err(harness.read(file, &mut tiny), Errno::EINVAL)?;

    let mut names = Vec::new();
    let mut one = [0u8; 24];
    loop {
        let length = harness.read(file, &mut one).map_err(|_| "read entry")?;
        if length == 0 {
            break;
        }
        let (entry, consumed) = DirEntry::decode(&one[..length]).map_err(|_| "decode entry")?;
        expect(consumed == length, "entry length")?;
        names.push(String::from_utf8(entry.name.to_vec()).map_err(|_| "utf8")?);
    }
    expect(names == ["a", "b", "c"], "pagination")?;

    // Mutation between reads invalidates the open-file-description cursor.
    let file = harness.open("/list", flags).map_err(|_| "open dir 2")?;
    let length = harness.read(file, &mut one).map_err(|_| "first entry")?;
    expect(length > 0, "no first entry")?;
    harness
        .fs()
        .create_file(dir, "d", 0o644)
        .map_err(|_| "create during read")?;
    expect_err(harness.read(file, &mut one), Errno::EINVAL)?;

    let file = harness.open("/list", flags).map_err(|_| "open dir 3")?;
    let length = harness.read(file, &mut one).map_err(|_| "first entry 2")?;
    expect(length > 0, "no first entry 2")?;
    harness
        .fs()
        .unlink(dir, "a")
        .map_err(|_| "unlink during read")?;
    expect_err(harness.read(file, &mut one), Errno::EINVAL)?;
    Ok(())
}

fn permission_contract() -> Result<(), &'static str> {
    let mut harness = Harness::new();
    let root = harness.fs().lookup("/").map_err(|_| "root")?;
    let ino = harness
        .fs()
        .create_file(root, "secret", 0o000)
        .map_err(|_| "create")?;
    harness
        .fs()
        .write_file(ino, b"readable")
        .map_err(|_| "write")?;
    let read = OpenFlags::from_linux(raven_proto_fs::O_RDONLY).map_err(|_| "read")?;
    let file = harness.open("/secret", read).map_err(|_| "mode 000 open")?;
    let mut buffer = [0u8; 8];
    expect(
        harness
            .read(file, &mut buffer)
            .map_err(|_| "mode 000 read")?
            == 8,
        "mode 000 read",
    )?;
    expect(&buffer == b"readable", "mode 000 data")?;
    let stat = harness.fstat(file).map_err(|_| "stat")?;
    expect(stat.mode & 0o777 == 0, "mode bits were not stored")?;
    expect(stat.uid == 0 && stat.gid == 0, "unexpected credentials")?;

    let write = OpenFlags::from_linux(raven_proto_fs::O_WRONLY).map_err(|_| "write")?;
    let writable = harness.open("/secret", write).map_err(|_| "write open")?;
    expect_err(harness.read(writable, &mut buffer), Errno::EBADF)?;
    let readable = harness.open("/secret", read).map_err(|_| "read open")?;
    expect_err(harness.write(readable, b"x"), Errno::EBADF)?;
    Ok(())
}

// `DirCookie` is the documented server-side cursor encoding; the fixture uses
// the OFD cursor directly, but a host test pins the encoding here.
#[cfg(test)]
mod tests {
    use super::*;
    use raven_proto_fs::DirCookie;

    #[test]
    fn contract_passes() {
        run_contract().expect("contract");
    }

    #[test]
    fn dir_cookie_encoding_is_stable() {
        let cookie = DirCookie::new(3, 9);
        assert_eq!(cookie.encode(), (3u64 << 32) | 9);
        assert_eq!(DirCookie::decode(cookie.encode()), cookie);
    }
}
