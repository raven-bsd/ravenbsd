// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S37 K66-conforming in-memory filesystem server.
//!
//! It uses `raven-scheme::OpenTable` for open-file descriptions, carries the typed `OpenFlags`,
//! and implements the sequential cursor, DUP/CLOSE reference counting, atomic O_EXCL/O_APPEND,
//! O_TRUNC, O_DIRECTORY, O_NOFOLLOW and unlinked-open lifetime. A control CALL reports the live
//! data-open count and the released-unlinked count for the qtest.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_proto_fs::{OpenFlags, S_IFDIR, S_IFMT, S_IFREG, Stat};
use raven_rt::entry;
use raven_rt::println;
use raven_s37_fs as _;
use raven_scheme::{Ctx, Handler, HandlerError, HandlerResult, OpenTable, Opened, Server};

entry!(main);

const ROOT: u64 = 1;

struct Inode {
    mode: u32,
    data: Vec<u8>,
    unlinked: bool,
    references: u32,
}

struct OpenState {
    ino: u64,
    control: bool,
}

struct Fs {
    inodes: BTreeMap<u64, Inode>,
    names: BTreeMap<String, u64>,
    opens: OpenTable<OpenState>,
    next_ino: u64,
    data_opens: usize,
    released: usize,
    offset_violations: usize,
}

impl Fs {
    fn new() -> Self {
        let mut inodes = BTreeMap::new();
        inodes.insert(
            ROOT,
            Inode {
                mode: S_IFDIR | 0o755,
                data: Vec::new(),
                unlinked: false,
                references: 0,
            },
        );
        let mut names = BTreeMap::new();
        names.insert("data".to_string(), 2);
        inodes.insert(
            2,
            Inode {
                mode: S_IFREG | 0o644,
                data: b"0123456789abcdef".to_vec(),
                unlinked: false,
                references: 0,
            },
        );
        names.insert("append".to_string(), 3);
        inodes.insert(
            3,
            Inode {
                mode: S_IFREG | 0o644,
                data: Vec::new(),
                unlinked: false,
                references: 0,
            },
        );
        names.insert("cycle".to_string(), 4);
        inodes.insert(
            4,
            Inode {
                mode: S_IFREG | 0o644,
                data: Vec::new(),
                unlinked: false,
                references: 0,
            },
        );
        names.insert("truncflag".to_string(), 6);
        inodes.insert(
            6,
            Inode {
                mode: S_IFREG | 0o644,
                data: b"truncate-me".to_vec(),
                unlinked: false,
                references: 0,
            },
        );
        names.insert("seq".to_string(), 7);
        inodes.insert(
            7,
            Inode {
                mode: S_IFREG | 0o644,
                data: Vec::new(),
                unlinked: false,
                references: 0,
            },
        );
        names.insert("linktarget".to_string(), 8);
        inodes.insert(
            8,
            Inode {
                mode: S_IFREG | 0o644,
                data: b"0123456789abcdef".to_vec(),
                unlinked: false,
                references: 0,
            },
        );
        names.insert("dir".to_string(), 5);
        inodes.insert(
            5,
            Inode {
                mode: S_IFDIR | 0o755,
                data: Vec::new(),
                unlinked: false,
                references: 0,
            },
        );
        Self {
            inodes,
            names,
            opens: OpenTable::new(),
            next_ino: 9,
            data_opens: 0,
            released: 0,
            offset_violations: 0,
        }
    }

    fn alloc_file(&mut self, data: Vec<u8>, unlinked: bool) -> u64 {
        let ino = self.next_ino;
        self.next_ino = self.next_ino.saturating_add(1);
        self.inodes.insert(
            ino,
            Inode {
                mode: S_IFREG | 0o644,
                data,
                unlinked,
                references: 0,
            },
        );
        ino
    }

    fn lookup(&self, path: &str) -> Result<u64, Errno> {
        if path.is_empty() || path == "." || path == ".." || path.contains("..") {
            return Err(Errno::EINVAL);
        }
        if path == "root" {
            return Ok(ROOT);
        }
        self.names.get(path).copied().ok_or(Errno::ENOENT)
    }

    fn is_dir(&self, ino: u64) -> Result<bool, Errno> {
        let inode = self.inodes.get(&ino).ok_or(Errno::ENOENT)?;
        Ok(inode.mode & S_IFMT == S_IFDIR)
    }

    fn open_ino(&mut self, ino: u64, flags: OpenFlags) -> HandlerResult<Opened> {
        let id = self
            .opens
            .insert(
                OpenState {
                    ino,
                    control: false,
                },
                flags,
            )
            .map_err(HandlerError::from)?;
        let inode = self.inodes.get_mut(&ino).ok_or(Errno::ENOENT)?;
        inode.references = inode.references.saturating_add(1);
        self.data_opens = self.data_opens.saturating_add(1);
        Ok(Opened::File(id))
    }

    fn open_existing(&mut self, path: &str, flags: OpenFlags) -> HandlerResult<Opened> {
        let ino = self.lookup(path).map_err(HandlerError::from)?;
        self.open_ino(ino, flags)
    }
}

impl Handler for Fs {
    fn open_with(
        &mut self,
        path: &str,
        flags: OpenFlags,
        mode: u32,
        _ctx: &Ctx,
    ) -> HandlerResult<Opened> {
        let path = path.strip_prefix('/').unwrap_or(path);
        if path == "ctl" {
            let id = self
                .opens
                .insert(
                    OpenState {
                        ino: 0,
                        control: true,
                    },
                    flags,
                )
                .map_err(HandlerError::from)?;
            return Ok(Opened::File(id));
        }
        if path == "unlinked" {
            let ino = self.alloc_file(b"unlinked-data".to_vec(), true);
            let id = self
                .opens
                .insert(
                    OpenState {
                        ino,
                        control: false,
                    },
                    flags,
                )
                .map_err(HandlerError::from)?;
            self.inodes.get_mut(&ino).ok_or(Errno::ENOENT)?.references = 1;
            self.data_opens = self.data_opens.saturating_add(1);
            return Ok(Opened::File(id));
        }
        if path == "link" {
            if flags.nofollow() {
                return Err(Errno::ELOOP.into());
            }
            return self.open_existing("linktarget", flags);
        }
        if path == "dir" {
            if flags.create() || flags.truncate() || flags.writable() {
                return Err(Errno::EISDIR.into());
            }
            if !flags.directory() {
                return Err(Errno::ENOTDIR.into());
            }
            return self.open_ino(ROOT + 4, flags);
        }
        let existing = self.lookup(path).ok();
        if let Some(ino) = existing {
            if flags.create() && flags.exclusive() {
                return Err(Errno::EEXIST.into());
            }
            if flags.directory() && !self.is_dir(ino).map_err(HandlerError::from)? {
                return Err(Errno::ENOTDIR.into());
            }
            if self.is_dir(ino).map_err(HandlerError::from)? && flags.writable() {
                return Err(Errno::EISDIR.into());
            }
            if flags.truncate() {
                self.inodes.get_mut(&ino).ok_or(Errno::ENOENT)?.data.clear();
            }
            return self.open_ino(ino, flags);
        }
        if !flags.create() {
            return Err(Errno::ENOENT.into());
        }
        let ino = self.alloc_file(Vec::new(), false);
        self.names.insert(path.to_string(), ino);
        let _ = mode;
        self.open_ino(ino, flags)
    }

    fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        if offset != 0 {
            self.offset_violations = self.offset_violations.saturating_add(1);
            return Err(Errno::EINVAL.into());
        }
        let (ino, cursor, readable, control) = {
            let open = self.opens.get(file).ok_or(Errno::EBADF)?;
            (
                open.file.ino,
                open.cursor(),
                open.flags.readable(),
                open.file.control,
            )
        };
        if control || !readable {
            return Err(Errno::EBADF.into());
        }
        if self.is_dir(ino).map_err(HandlerError::from)? {
            return Err(Errno::EISDIR.into());
        }
        let data = &self.inodes.get(&ino).ok_or(Errno::ENOENT)?.data;
        let start = usize::try_from(cursor)
            .unwrap_or(usize::MAX)
            .min(data.len());
        let available = data.len().saturating_sub(start);
        let count = buffer.len().min(available);
        if let Some(destination) = buffer.get_mut(..count)
            && let Some(source) = data.get(start..start.saturating_add(count))
        {
            destination.copy_from_slice(source);
        }
        self.opens
            .get_mut(file)
            .ok_or(Errno::EBADF)?
            .advance(count)
            .map_err(HandlerError::from)?;
        Ok(count)
    }

    fn write(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
        if offset != 0 {
            self.offset_violations = self.offset_violations.saturating_add(1);
            return Err(Errno::EINVAL.into());
        }
        let (ino, cursor, append, writable, control) = {
            let open = self.opens.get(file).ok_or(Errno::EBADF)?;
            (
                open.file.ino,
                open.cursor(),
                open.flags.append(),
                open.flags.writable(),
                open.file.control,
            )
        };
        if control || !writable {
            return Err(Errno::EBADF.into());
        }
        if self.is_dir(ino).map_err(HandlerError::from)? {
            return Err(Errno::EISDIR.into());
        }
        let inode = self.inodes.get_mut(&ino).ok_or(Errno::ENOENT)?;
        let position = if append {
            inode.data.len()
        } else {
            usize::try_from(cursor)
                .unwrap_or(usize::MAX)
                .min(inode.data.len())
        };
        let end = position.checked_add(buffer.len()).ok_or(Errno::EOVERFLOW)?;
        if end > inode.data.len() {
            inode.data.resize(end, 0);
        }
        if let Some(destination) = inode.data.get_mut(position..end) {
            destination.copy_from_slice(buffer);
        }
        self.opens
            .get_mut(file)
            .ok_or(Errno::EBADF)?
            .advance(buffer.len())
            .map_err(HandlerError::from)?;
        Ok(buffer.len())
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        let state = self.opens.close(file).map_err(HandlerError::from)?;
        if let Some(state) = state {
            if state.control {
                return Ok(());
            }
            self.data_opens = self.data_opens.saturating_sub(1);
            let release = {
                let inode = self.inodes.get_mut(&state.ino).ok_or(Errno::ENOENT)?;
                inode.references = inode.references.saturating_sub(1);
                inode.unlinked && inode.references == 0
            };
            if release {
                self.inodes.remove(&state.ino);
                self.released = self.released.saturating_add(1);
            }
        }
        Ok(())
    }

    fn dup(&mut self, file: u64) -> HandlerResult<u64> {
        let id = self.opens.dup(file).map_err(HandlerError::from)?;
        Ok(id)
    }

    fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
        let open = self.opens.get(file).ok_or(Errno::EBADF)?;
        let inode = self.inodes.get(&open.file.ino).ok_or(Errno::ENOENT)?;
        let size = inode.data.len() as u64;
        Ok(Stat {
            ino: open.file.ino,
            size,
            blocks: size.div_ceil(512),
            mtime: 0,
            ctime: 0,
            atime: 0,
            mode: inode.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
        })
    }

    fn call(&mut self, _file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        let text: String = if request == b"count" {
            format!(
                "opens={} released={} offsets={}",
                self.data_opens, self.released, self.offset_violations
            )
        } else {
            "ok".to_string()
        };
        let bytes = text.as_bytes();
        if bytes.len() > response.len() {
            return Err(Errno::E2BIG.into());
        }
        if let Some(destination) = response.get_mut(..bytes.len()) {
            destination.copy_from_slice(bytes);
        }
        Ok(bytes.len())
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let mut server = match Server::register("fs") {
        Ok(server) => server,
        Err(_) => {
            println!("s37: server register failed");
            return 1;
        }
    };
    let mut fs = Fs::new();
    println!("s37: server ready");
    server.run(&mut fs)
}
