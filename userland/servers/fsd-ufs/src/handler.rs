// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The read-only filesystem handler.
//!
//! The handler owns the open-file table and turns S11 operations into calls on
//! a [`FileSystem`].  It is independent of UFS and of the target, so the
//! directory-cursor and error rules are covered by host tests.

use alloc::string::{String, ToString};
use raven_abi::errno::Errno;
use raven_proto_fs::{
    Call, CallRequest, CallResponse, MAX_PATH, OpenFlags, S_IFDIR, S_IFMT, Stat, StatFs,
};
use raven_scheme::{Ctx, Handler, HandlerResult, OpenTable, Opened};

/// Aggregate open-file-description budget.
pub const MAX_OPEN_FILES: usize = 256;

/// The filesystem operations the handler needs.
///
/// The inode type is an opaque `u32` to the handler; only the filesystem
/// adapter interprets it.
pub trait FileSystem {
    /// Resolve a path, following a final symbolic link.
    fn resolve(&mut self, path: &str) -> Result<u32, Errno>;

    /// Resolve a path without following a final symbolic link.
    fn resolve_nofollow(&mut self, path: &str) -> Result<u32, Errno>;

    /// Read inode attributes.
    fn stat(&mut self, ino: u32) -> Result<Stat, Errno>;

    /// Read file data at `offset`; a short read is end-of-file.
    fn read(&mut self, ino: u32, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno>;

    /// Read the next packed directory entries from `cookie`; returns bytes and
    /// the next cookie.
    fn readdir(&mut self, ino: u32, cookie: u64, buffer: &mut [u8]) -> Result<(usize, u64), Errno>;

    /// Read a symbolic-link target.
    fn readlink(&mut self, ino: u32, buffer: &mut [u8]) -> Result<usize, Errno>;

    /// Filesystem statistics.
    fn statfs(&self) -> StatFs;
}

struct OpenState {
    ino: u32,
    dir_cookie: u64,
}

/// A stateful read-only filesystem handler.
pub struct FsHandler<F: FileSystem> {
    fs: F,
    opens: OpenTable<OpenState>,
    pending: Option<(u64, String)>,
}

impl<F: FileSystem> FsHandler<F> {
    /// Create a handler over `fs`.
    pub fn new(fs: F) -> Self {
        Self {
            fs,
            opens: OpenTable::new(),
            pending: None,
        }
    }

    /// Record the kernel file id and path of the next `OPEN`.
    ///
    /// `raven-scheme` does not pass the kernel file id to the handler, so the
    /// server loop and host tests set this before dispatch.
    pub fn expect_open(&mut self, file_id: u64, path: &str) {
        self.pending = Some((file_id, path.to_string()));
    }

    fn status(&self, response: &mut [u8], status: i32) -> HandlerResult<usize> {
        CallResponse::Status(status)
            .encode(response)
            .map_err(|_| Errno::EPROTO.into())
    }
}

fn bytes_to_string(bytes: &[u8]) -> Result<&str, Errno> {
    core::str::from_utf8(bytes).map_err(|_| Errno::EINVAL)
}

impl<F: FileSystem> Handler for FsHandler<F> {
    fn open_with(
        &mut self,
        path: &str,
        flags: OpenFlags,
        _mode: u32,
        _ctx: &Ctx,
    ) -> HandlerResult<Opened> {
        let Some((file_id, expected)) = self.pending.take() else {
            return Err(Errno::EINVAL.into());
        };
        if expected != path {
            return Err(Errno::EINVAL.into());
        }
        if flags.writable() || flags.create() || flags.truncate() || flags.append() {
            return Err(Errno::EROFS.into());
        }
        if self.opens.len() >= MAX_OPEN_FILES {
            return Err(Errno::EMFILE.into());
        }
        let ino = self.fs.resolve(path)?;
        let stat = self.fs.stat(ino)?;
        if flags.directory() && stat.mode & S_IFMT != S_IFDIR {
            return Err(Errno::ENOTDIR.into());
        }
        self.opens
            .insert_with_id(file_id, OpenState { ino, dir_cookie: 0 }, flags)?;
        Ok(Opened::File(file_id))
    }

    fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        if offset != 0 {
            return Err(Errno::EINVAL.into());
        }
        let (ino, cursor, dir_cookie, flags) = {
            let open = self.opens.get(file).ok_or(Errno::EBADF)?;
            (
                open.file.ino,
                open.cursor(),
                open.file.dir_cookie,
                open.flags,
            )
        };
        if !flags.readable() {
            return Err(Errno::EBADF.into());
        }
        let stat = self.fs.stat(ino)?;
        if stat.mode & S_IFMT == S_IFDIR {
            let (read, next) = self.fs.readdir(ino, dir_cookie, buffer)?;
            let open = self.opens.get_mut(file).ok_or(Errno::EBADF)?;
            open.file.dir_cookie = next;
            Ok(read)
        } else {
            let read = self.fs.read(ino, cursor, buffer)?;
            let open = self.opens.get_mut(file).ok_or(Errno::EBADF)?;
            let cursor = open.cursor().checked_add(read as u64).ok_or(Errno::EFBIG)?;
            open.set_cursor(cursor);
            Ok(read)
        }
    }

    fn write(&mut self, file: u64, _offset: u64, _buffer: &[u8]) -> HandlerResult<usize> {
        if self.opens.get(file).is_none() {
            return Err(Errno::EBADF.into());
        }
        Err(Errno::EROFS.into())
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        self.opens.close(file)?;
        Ok(())
    }

    fn dup(&mut self, file: u64) -> HandlerResult<u64> {
        Ok(self.opens.dup(file)?)
    }

    fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
        let ino = self.opens.get(file).ok_or(Errno::EBADF)?.file.ino;
        Ok(self.fs.stat(ino)?)
    }

    fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        if self.opens.get(file).is_none() {
            return Err(Errno::EBADF.into());
        }
        let decoded = CallRequest::decode(request).map_err(|_| Errno::EPROTO)?;
        match decoded.call {
            Call::Readlink { path } => {
                let path = bytes_to_string(path)?;
                let ino = match self.fs.resolve_nofollow(path) {
                    Ok(ino) => ino,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                let mut target = [0u8; MAX_PATH + 1];
                let length = match self.fs.readlink(ino, &mut target) {
                    Ok(length) => length,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                if length == 0 || length > MAX_PATH {
                    return self.status(response, -i32::from(Errno::ENAMETOOLONG.0));
                }
                CallResponse::Readlink {
                    status: 0,
                    target: &target[..length],
                }
                .encode(response)
                .map_err(|_| Errno::EPROTO.into())
            }
            Call::Statfs => CallResponse::StatFs {
                status: 0,
                statfs: self.fs.statfs(),
            }
            .encode(response)
            .map_err(|_| Errno::EPROTO.into()),
            Call::Sync => self.status(response, 0),
            Call::Mkdir { .. }
            | Call::Unlink { .. }
            | Call::Rmdir { .. }
            | Call::Rename { .. }
            | Call::Truncate { .. }
            | Call::Symlink { .. }
            | Call::Chmod { .. } => self.status(response, -i32::from(Errno::EROFS.0)),
            Call::Mount { .. } | Call::Umount { .. } => {
                self.status(response, -i32::from(Errno::EINVAL.0))
            }
        }
    }
}

#[cfg(test)]
#[path = "handler_ut.rs"]
mod tests;
