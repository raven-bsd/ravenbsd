// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The read-only filesystem handler.
//!
//! The handler owns the open-file table and turns S11 operations into calls on
//! a [`FileSystem`].  It is independent of UFS and of the target, so the
//! directory-cursor and error rules are covered by host tests.

use alloc::vec::Vec;
use raven_abi::errno::Errno;
use raven_proto_fs::{
    ACCESS_EXEC, ACCESS_READ, ACCESS_WRITE, Call, CallRequest, CallResponse, Credentials, MAX_PATH,
    OpenFlags, S_IFDIR, S_IFMT, SetTime, Stat, StatFs, StatVfs,
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
    fn resolve(&mut self, path: &[u8]) -> Result<u32, Errno>;

    /// Resolve a path without following a final symbolic link.
    fn resolve_nofollow(&mut self, path: &[u8]) -> Result<u32, Errno>;

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
    fn create_file(&mut self, path: &[u8], mode: u16, exclusive: bool) -> Result<u32, Errno> {
        let _ = (path, mode, exclusive);
        Err(Errno::EROFS)
    }

    /// Create a directory.
    fn mkdir(&mut self, path: &[u8], mode: u16) -> Result<(), Errno> {
        let _ = (path, mode);
        Err(Errno::EROFS)
    }

    /// Remove one non-directory entry.
    fn unlink(&mut self, path: &[u8]) -> Result<(), Errno> {
        let _ = path;
        Err(Errno::EROFS)
    }

    /// Remove one empty directory.
    fn rmdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        let _ = path;
        Err(Errno::EROFS)
    }

    /// Rename one entry.
    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        let _ = (from, to);
        Err(Errno::EROFS)
    }

    /// Create a symbolic link.
    fn symlink(&mut self, path: &[u8], target: &[u8]) -> Result<(), Errno> {
        let _ = (path, target);
        Err(Errno::EROFS)
    }

    /// Create a hard link.
    fn link(&mut self, path: &[u8], target: &[u8]) -> Result<(), Errno> {
        let _ = (path, target);
        Err(Errno::EROFS)
    }

    /// Change permission bits.
    fn chmod(&mut self, path: &[u8], mode: u16) -> Result<(), Errno> {
        let _ = (path, mode);
        Err(Errno::EROFS)
    }

    /// Change ownership of `path`, optionally following a final symlink.
    fn chown(&mut self, path: &[u8], uid: u32, gid: u32, follow: bool) -> Result<(), Errno> {
        let _ = (path, uid, gid, follow);
        Err(Errno::EROFS)
    }

    /// Change ownership of an open inode.
    fn fchown(&mut self, ino: u32, uid: u32, gid: u32) -> Result<(), Errno> {
        let _ = (ino, uid, gid);
        Err(Errno::EROFS)
    }

    /// Set atime/mtime of `path`; `None` leaves that timestamp unchanged.
    fn utimens(
        &mut self,
        path: &[u8],
        atime: Option<i64>,
        mtime: Option<i64>,
        follow: bool,
    ) -> Result<(), Errno> {
        let _ = (path, atime, mtime, follow);
        Err(Errno::EROFS)
    }

    /// Create a FIFO.
    fn mkfifo(&mut self, path: &[u8], mode: u16) -> Result<(), Errno> {
        let _ = (path, mode);
        Err(Errno::EROFS)
    }

    /// Create a special node; `mode` carries the file type.
    fn mknod(&mut self, path: &[u8], mode: u16, dev: u32) -> Result<(), Errno> {
        let _ = (path, mode, dev);
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
    credentials: Credentials,
}

/// A stateful read-only filesystem handler.
pub struct FsHandler<F: FileSystem> {
    fs: F,
    opens: OpenTable<OpenState>,
    pending: Option<(u64, Vec<u8>)>,
    now: fn() -> i64,
}

impl<F: FileSystem> FsHandler<F> {
    /// Create a handler over `fs` with a zero clock.
    pub fn new(fs: F) -> Self {
        Self::with_clock(fs, || 0)
    }

    /// Create a handler over `fs` with an explicit wall clock.
    pub fn with_clock(fs: F, now: fn() -> i64) -> Self {
        Self {
            fs,
            opens: OpenTable::new(),
            pending: None,
            now,
        }
    }

    fn clock(&self) -> i64 {
        (self.now)()
    }

    /// Record the kernel file id and path of the next `OPEN`.
    ///
    /// `raven-scheme` does not pass the kernel file id to the handler, so the
    /// server loop and host tests set this before dispatch.
    pub fn expect_open(&mut self, file_id: u64, path: &[u8]) {
        self.pending = Some((file_id, path.to_vec()));
    }

    fn status(&self, response: &mut [u8], status: i32) -> HandlerResult<usize> {
        CallResponse::Status(status)
            .encode(response)
            .map_err(|_| Errno::EPROTO.into())
    }

    fn check_traverse(&mut self, path: &[u8], creds: &Credentials) -> Result<(), Errno> {
        let absolute = path.first() == Some(&b'/');
        let mut prefix = Vec::new();
        if absolute {
            prefix.push(b'/');
        }
        for component in path
            .split(|byte| *byte == b'/')
            .filter(|part| !part.is_empty())
        {
            if !prefix.is_empty() && prefix.last() != Some(&b'/') {
                prefix.push(b'/');
            }
            prefix.extend_from_slice(component);
            let ino = self.fs.resolve(&prefix)?;
            let stat = self.fs.stat(ino)?;
            if stat.mode & S_IFMT == S_IFDIR
                && !creds.permitted((stat.mode & 0o7777) as u16, stat.uid, stat.gid, ACCESS_EXEC)
            {
                return Err(Errno::EACCES);
            }
        }
        Ok(())
    }

    fn result(&self, response: &mut [u8], result: Result<(), Errno>) -> HandlerResult<usize> {
        match result {
            Ok(()) => self.status(response, 0),
            Err(error) => self.status(response, -i32::from(error.0)),
        }
    }
}

fn parent_path(path: &[u8]) -> &[u8] {
    let trimmed = path.strip_suffix(b"/").unwrap_or(path);
    match trimmed.iter().rposition(|byte| *byte == b'/') {
        Some(index) if index > 0 => &trimmed[..index],
        _ => b"/",
    }
}

/// POSIX ownership rules: root may change anything; an owner may keep the uid and select a group
/// they belong to.
fn check_chown(creds: &Credentials, stat: &Stat, uid: u32, gid: u32) -> Result<(), Errno> {
    if creds.is_root() {
        return Ok(());
    }
    if creds.euid != stat.uid {
        return Err(Errno::EPERM);
    }
    if uid != u32::MAX && uid != stat.uid {
        return Err(Errno::EPERM);
    }
    if gid != u32::MAX && gid != stat.gid && !creds.in_group(gid) {
        return Err(Errno::EPERM);
    }
    Ok(())
}

/// Arbitrary timestamps need ownership; `UTIME_NOW`/`UTIME_OMIT` also accept write permission.
fn check_utimens(creds: &Credentials, stat: &Stat, explicit: bool) -> Result<(), Errno> {
    if creds.is_root() || creds.euid == stat.uid {
        return Ok(());
    }
    if explicit {
        return Err(Errno::EPERM);
    }
    if creds.permitted(
        (stat.mode & 0o7777) as u16,
        stat.uid,
        stat.gid,
        ACCESS_WRITE,
    ) {
        Ok(())
    } else {
        Err(Errno::EACCES)
    }
}

impl<F: FileSystem> Handler for FsHandler<F> {
    fn open_with(
        &mut self,
        path: &str,
        flags: OpenFlags,
        _mode: u32,
        ctx: &Ctx,
    ) -> HandlerResult<Opened> {
        let Some((file_id, expected)) = self.pending.take() else {
            return Err(Errno::EINVAL.into());
        };
        let path = path.as_bytes();
        if expected.as_slice() != path {
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
            self.check_traverse(path, &ctx.credentials)?;
            self.fs.resolve(path)?
        };
        if flags.truncate() {
            self.fs.truncate(ino, 0)?;
        }
        let stat = self.fs.stat(ino)?;
        if flags.directory() && stat.mode & S_IFMT != S_IFDIR {
            return Err(Errno::ENOTDIR.into());
        }
        let mut want = 0;
        if flags.readable() {
            want |= ACCESS_READ;
        }
        if flags.writable() || flags.truncate() {
            want |= ACCESS_WRITE;
        }
        if stat.mode & S_IFMT == S_IFDIR && flags.readable() {
            want |= ACCESS_EXEC;
        }
        if !ctx
            .credentials
            .permitted((stat.mode & 0o7777) as u16, stat.uid, stat.gid, want)
        {
            return Err(Errno::EACCES.into());
        }
        let cursor = if flags.append() { stat.size } else { 0 };
        self.opens.insert_with_id(
            file_id,
            OpenState {
                ino,
                dir_cookie: 0,
                credentials: ctx.credentials,
            },
            flags,
        )?;
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

    fn pread(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        let (ino, flags) = {
            let open = self.opens.get(file).ok_or(Errno::EBADF)?;
            (open.file.ino, open.flags)
        };
        if !flags.readable() {
            return Err(Errno::EBADF.into());
        }
        let stat = self.fs.stat(ino)?;
        if stat.mode & S_IFMT == S_IFDIR {
            return Err(Errno::EISDIR.into());
        }
        Ok(self.fs.read(ino, offset, buffer)?)
    }

    fn pwrite(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
        let (ino, flags) = {
            let open = self.opens.get(file).ok_or(Errno::EBADF)?;
            (open.file.ino, open.flags)
        };
        if !flags.writable() {
            return Err(Errno::EBADF.into());
        }
        if self.fs.is_read_only() {
            return Err(Errno::EROFS.into());
        }
        let stat = self.fs.stat(ino)?;
        if stat.mode & S_IFMT == S_IFDIR {
            return Err(Errno::EISDIR.into());
        }
        Ok(self.fs.write(ino, offset, buffer)?)
    }

    fn seek(&mut self, file: u64, offset: i64, whence: u32) -> HandlerResult<u64> {
        let (ino, cursor) = {
            let open = self.opens.get(file).ok_or(Errno::EBADF)?;
            (open.file.ino, open.cursor())
        };
        let size = if whence == 2 {
            self.fs.stat(ino)?.size
        } else {
            0
        };
        let base = match whence {
            0 => 0,
            1 => cursor,
            2 => size,
            _ => return Err(Errno::EINVAL.into()),
        };
        let target = i128::from(base)
            .checked_add(i128::from(offset))
            .ok_or(Errno::EINVAL)?;
        if target < 0 || target > u64::MAX as i128 {
            return Err(Errno::EINVAL.into());
        }
        let target = target as u64;
        self.opens
            .get_mut(file)
            .ok_or(Errno::EBADF)?
            .set_cursor(target);
        Ok(target)
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
        let credentials = self.opens.get(file).ok_or(Errno::EBADF)?.file.credentials;
        let decoded = CallRequest::decode(request).map_err(|_| Errno::EPROTO)?;
        match decoded.call {
            Call::Readlink { path } => {
                if let Err(error) = self.check_traverse(parent_path(path), &credentials) {
                    return self.status(response, -i32::from(error.0));
                }
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
            Call::Lstat { path } => {
                if let Err(error) = self.check_traverse(parent_path(path), &credentials) {
                    return self.status(response, -i32::from(error.0));
                }
                let ino = match self.fs.resolve_nofollow(path) {
                    Ok(ino) => ino,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                let stat = match self.fs.stat(ino) {
                    Ok(stat) => stat,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                CallResponse::Stat { status: 0, stat }
                    .encode(response)
                    .map_err(|_| Errno::EPROTO.into())
            }
            Call::Sync => {
                let result = self.fs.sync();
                self.result(response, result)
            }
            Call::Mkdir { mode, path } => {
                let mode = u16::try_from(mode & 0o7777).map_err(|_| Errno::EINVAL)?;
                let result = self.fs.mkdir(path, mode);
                self.result(response, result)
            }
            Call::Unlink { path } => {
                let result = self.fs.unlink(path);
                self.result(response, result)
            }
            Call::Rmdir { path } => {
                let result = self.fs.rmdir(path);
                self.result(response, result)
            }
            Call::Rename { from, to } => {
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
                let result = self.fs.symlink(path, target);
                self.result(response, result)
            }
            Call::Chmod { mode, path } => {
                let mode = u16::try_from(mode & 0o7777).map_err(|_| Errno::EINVAL)?;
                let result = self.fs.chmod(path, mode);
                self.result(response, result)
            }
            Call::Link { path, target } => {
                if let Err(error) = self.check_traverse(parent_path(path), &credentials) {
                    return self.status(response, -i32::from(error.0));
                }
                let result = self.fs.link(path, target);
                self.result(response, result)
            }
            Call::Chown { uid, gid, path } | Call::Lchown { uid, gid, path } => {
                let follow = matches!(decoded.call, Call::Chown { .. });
                if let Err(error) = self.check_traverse(path, &credentials) {
                    return self.status(response, -i32::from(error.0));
                }
                let ino = match if follow {
                    self.fs.resolve(path)
                } else {
                    self.fs.resolve_nofollow(path)
                } {
                    Ok(ino) => ino,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                let stat = match self.fs.stat(ino) {
                    Ok(stat) => stat,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                if let Err(error) = check_chown(&credentials, &stat, uid, gid) {
                    return self.status(response, -i32::from(error.0));
                }
                let result = self.fs.chown(path, uid, gid, follow);
                self.result(response, result)
            }
            Call::Fchown { uid, gid } => {
                let ino = self.opens.get(file).ok_or(Errno::EBADF)?.file.ino;
                let stat = match self.fs.stat(ino) {
                    Ok(stat) => stat,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                if let Err(error) = check_chown(&credentials, &stat, uid, gid) {
                    return self.status(response, -i32::from(error.0));
                }
                let result = self.fs.fchown(ino, uid, gid);
                self.result(response, result)
            }
            Call::Mkfifo { mode, path } => {
                let mode = u16::try_from(mode & 0o7777).map_err(|_| Errno::EINVAL)?;
                if let Err(error) = self.check_traverse(parent_path(path), &credentials) {
                    return self.status(response, -i32::from(error.0));
                }
                let result = self.fs.mkfifo(path, mode);
                self.result(response, result)
            }
            Call::Mknod { mode, dev, path } => {
                let mode = u16::try_from(mode).map_err(|_| Errno::EINVAL)?;
                if let Err(error) = self.check_traverse(parent_path(path), &credentials) {
                    return self.status(response, -i32::from(error.0));
                }
                let result = self.fs.mknod(path, mode, dev);
                self.result(response, result)
            }
            Call::Utimens {
                path,
                atime,
                mtime,
                follow,
            } => {
                if let Err(error) = self.check_traverse(path, &credentials) {
                    return self.status(response, -i32::from(error.0));
                }
                let ino = match if follow {
                    self.fs.resolve(path)
                } else {
                    self.fs.resolve_nofollow(path)
                } {
                    Ok(ino) => ino,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                let stat = match self.fs.stat(ino) {
                    Ok(stat) => stat,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                let explicit = matches!(atime, SetTime::Set(_)) || matches!(mtime, SetTime::Set(_));
                if let Err(error) = check_utimens(&credentials, &stat, explicit) {
                    return self.status(response, -i32::from(error.0));
                }
                let clock = self.clock();
                let atime = match atime {
                    SetTime::Set(seconds) => Some(seconds),
                    SetTime::Now => Some(clock),
                    SetTime::Omit => None,
                };
                let mtime = match mtime {
                    SetTime::Set(seconds) => Some(seconds),
                    SetTime::Now => Some(clock),
                    SetTime::Omit => None,
                };
                let result = self.fs.utimens(path, atime, mtime, follow);
                self.result(response, result)
            }
            Call::Statvfs => {
                let statvfs: StatVfs = self.fs.statfs().into();
                CallResponse::StatVfs { status: 0, statvfs }
                    .encode(response)
                    .map_err(|_| Errno::EPROTO.into())
            }
            Call::Mount { .. } | Call::Umount { .. } | Call::Mmap { .. } => {
                self.status(response, -i32::from(Errno::EINVAL.0))
            }
            Call::Access { mode, path } => {
                if let Err(error) = self.check_traverse(path, &credentials) {
                    return self.status(response, -i32::from(error.0));
                }
                let ino = match self.fs.resolve(path) {
                    Ok(ino) => ino,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                let stat = match self.fs.stat(ino) {
                    Ok(stat) => stat,
                    Err(error) => return self.status(response, -i32::from(error.0)),
                };
                let allowed = credentials.permitted(
                    (stat.mode & 0o7777) as u16,
                    stat.uid,
                    stat.gid,
                    mode as u16,
                );
                self.status(
                    response,
                    if allowed {
                        0
                    } else {
                        -i32::from(Errno::EACCES.0)
                    },
                )
            }
        }
    }
}

#[cfg(test)]
#[path = "handler_ut.rs"]
mod tests;
