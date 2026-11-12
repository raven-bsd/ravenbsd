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

    /// Whether the mount is currently read-only.
    fn is_read_only(&self) -> bool {
        true
    }

    /// Create a regular file at `path`; when `exclusive` is set an existing
    /// file is an error.
    fn create_file(&mut self, path: &str, mode: u16, exclusive: bool) -> Result<u32, Errno> {
        let _ = (path, mode, exclusive);
        Err(Errno::EROFS)
    }

    /// Create a directory.
    fn mkdir(&mut self, path: &str, mode: u16) -> Result<(), Errno> {
        let _ = (path, mode);
        Err(Errno::EROFS)
    }

    /// Remove one non-directory entry.
    fn unlink(&mut self, path: &str) -> Result<(), Errno> {
        let _ = path;
        Err(Errno::EROFS)
    }

    /// Remove one empty directory.
    fn rmdir(&mut self, path: &str) -> Result<(), Errno> {
        let _ = path;
        Err(Errno::EROFS)
    }

    /// Rename one entry.
    fn rename(&mut self, from: &str, to: &str) -> Result<(), Errno> {
        let _ = (from, to);
        Err(Errno::EROFS)
    }

    /// Create a symbolic link.
    fn symlink(&mut self, path: &str, target: &[u8]) -> Result<(), Errno> {
        let _ = (path, target);
        Err(Errno::EROFS)
    }

    /// Create a hard link.
    fn link(&mut self, path: &str, target: &str) -> Result<(), Errno> {
        let _ = (path, target);
        Err(Errno::EROFS)
    }

    /// Change permission bits.
    fn chmod(&mut self, path: &str, mode: u16) -> Result<(), Errno> {
        let _ = (path, mode);
        Err(Errno::EROFS)
    }

    /// Change the open file's length.
    fn truncate(&mut self, ino: u32, size: u64) -> Result<(), Errno> {
        let _ = (ino, size);
        Err(Errno::EROFS)
    }

    /// Write file data.
    fn write(&mut self, ino: u32, offset: u64, data: &[u8]) -> Result<usize, Errno> {
        let _ = (ino, offset, data);
        Err(Errno::EROFS)
    }

    /// Flush the whole filesystem and the block device.
    fn sync(&mut self) -> Result<(), Errno> {
        Ok(())
    }
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

    fn result(&self, response: &mut [u8], result: Result<(), Errno>) -> HandlerResult<usize> {
        match result {
            Ok(()) => self.status(response, 0),
            Err(error) => self.status(response, -i32::from(error.0)),
        }
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
        let mutating = flags.writable() || flags.create() || flags.truncate() || flags.append();
        if mutating && self.fs.is_read_only() {
            return Err(Errno::EROFS.into());
        }
        if self.opens.len() >= MAX_OPEN_FILES {
            return Err(Errno::EMFILE.into());
        }
        let ino = if flags.create() {
            let mode = u16::try_from(flags.create_mode().unwrap_or(0o644) & 0o7777)
                .map_err(|_| Errno::EINVAL)?;
            self.fs.create_file(path, mode, flags.exclusive())?
        } else {
            self.fs.resolve(path)?
        };
        if flags.truncate() {
            self.fs.truncate(ino, 0)?;
        }
        let stat = self.fs.stat(ino)?;
        if flags.directory() && stat.mode & S_IFMT != S_IFDIR {
            return Err(Errno::ENOTDIR.into());
        }
        let cursor = if flags.append() { stat.size } else { 0 };
        self.opens
            .insert_with_id(file_id, OpenState { ino, dir_cookie: 0 }, flags)?;
        if let Some(open) = self.opens.get_mut(file_id) {
            open.set_cursor(cursor);
        }
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

    fn write(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
        if offset != 0 {
            return Err(Errno::EINVAL.into());
        }
        let (ino, cursor, flags) = {
            let open = self.opens.get(file).ok_or(Errno::EBADF)?;
            (open.file.ino, open.cursor(), open.flags)
        };
        if !flags.writable() {
            return Err(Errno::EBADF.into());
        }
        if self.fs.is_read_only() {
            return Err(Errno::EROFS.into());
        }
        let written = self.fs.write(ino, cursor, buffer)?;
        let open = self.opens.get_mut(file).ok_or(Errno::EBADF)?;
        let cursor = open
            .cursor()
            .checked_add(written as u64)
            .ok_or(Errno::EFBIG)?;
        open.set_cursor(cursor);
        Ok(written)
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
            Call::Sync => {
                let result = self.fs.sync();
                self.result(response, result)
            }
            Call::Mkdir { mode, path } => {
                let path = bytes_to_string(path)?;
                let mode = u16::try_from(mode & 0o7777).map_err(|_| Errno::EINVAL)?;
                let result = self.fs.mkdir(path, mode);
                self.result(response, result)
            }
            Call::Unlink { path } => {
                let path = bytes_to_string(path)?;
                let result = self.fs.unlink(path);
                self.result(response, result)
            }
            Call::Rmdir { path } => {
                let path = bytes_to_string(path)?;
                let result = self.fs.rmdir(path);
                self.result(response, result)
            }
            Call::Rename { from, to } => {
                let from = bytes_to_string(from)?;
                let to = bytes_to_string(to)?;
                let result = self.fs.rename(from, to);
                self.result(response, result)
            }
            Call::Truncate { size } => {
                let (ino, flags) = {
                    let open = self.opens.get(file).ok_or(Errno::EBADF)?;
                    (open.file.ino, open.flags)
                };
                if !flags.writable() {
                    return self.status(response, -i32::from(Errno::EBADF.0));
                }
                let result = self.fs.truncate(ino, size);
                if result.is_ok()
                    && let Some(open) = self.opens.get_mut(file)
                    && open.cursor() > size
                {
                    open.set_cursor(size);
                }
                self.result(response, result)
            }
            Call::Symlink { path, target } => {
                let path = bytes_to_string(path)?;
                let result = self.fs.symlink(path, target);
                self.result(response, result)
            }
            Call::Chmod { mode, path } => {
                let path = bytes_to_string(path)?;
                let mode = u16::try_from(mode & 0o7777).map_err(|_| Errno::EINVAL)?;
                let result = self.fs.chmod(path, mode);
                self.result(response, result)
            }
            Call::Mount { .. } | Call::Umount { .. } => {
                self.status(response, -i32::from(Errno::EINVAL.0))
            }
        }
    }
}

#[cfg(test)]
#[path = "handler_ut.rs"]
mod tests;
