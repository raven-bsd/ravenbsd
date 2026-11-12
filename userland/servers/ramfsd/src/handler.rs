// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stateful `ramfs:` handler.
//!
//! This module is deliberately independent of the target runtime: the caller
//! supplies the clock, so the server logic can run under `cargo test` with a
//! fake kernel.  The real `main` in `main.rs` starts it with the syscall clock.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

use raven_abi::errno::Errno;
use raven_proto_fs::{
    Call, CallRequest, CallResponse, OpenFlags, S_IFDIR, S_IFLNK, S_IFMT, Stat, StatFs,
};
use raven_ramfs_core::{ROOT_INO, Ramfs};
use raven_scheme::{Ctx, Handler, HandlerResult, OpenTable, Opened};

/// Aggregate open-file-description budget.
pub const MAX_OPEN_FILES: usize = 256;

/// One open-file description's server state.
struct OpenState {
    ino: u64,
    dir_generation: Option<u32>,
}

/// The ramfs server handler.
pub struct RamfsHandler {
    fs: Ramfs,
    opens: OpenTable<OpenState>,
    refs: BTreeMap<u64, u32>,
    dir_generations: BTreeMap<u64, u32>,
    pending: Option<(u64, String)>,
    now: fn() -> i64,
}

impl RamfsHandler {
    /// Create a handler over a fresh ramfs.
    pub fn new(now: fn() -> i64) -> Self {
        Self {
            fs: Ramfs::new(now()),
            opens: OpenTable::new(),
            refs: BTreeMap::new(),
            dir_generations: BTreeMap::new(),
            pending: None,
            now,
        }
    }

    /// Record the kernel file id and path of the next `OPEN`.
    ///
    /// `raven-scheme` does not pass the kernel file id to the handler, so the
    /// server loop and host tests set this before dispatch.
    pub fn expect_open(&mut self, file_id: u64, path: &str) {
        self.pending = Some((file_id, path.to_string()));
    }

    fn clock(&self) -> i64 {
        (self.now)()
    }

    fn reference(&mut self, ino: u64) -> Result<(), Errno> {
        let count = self.refs.entry(ino).or_insert(0);
        *count = count.checked_add(1).ok_or(Errno::EMFILE)?;
        Ok(())
    }

    fn dereference(&mut self, ino: u64) {
        let Some(count) = self.refs.get_mut(&ino) else {
            return;
        };
        if *count > 1 {
            *count -= 1;
            return;
        }
        self.refs.remove(&ino);
        if self.fs.is_unlinked(ino) {
            let _ = self.fs.release(ino);
        }
    }

    fn release_if_unreferenced(&mut self, ino: u64) {
        if self.refs.get(&ino).copied().unwrap_or(0) == 0 {
            let _ = self.fs.release(ino);
        }
    }

    fn bump_dir(&mut self, ino: u64) {
        let generation = self.dir_generations.entry(ino).or_insert(0);
        *generation = generation.wrapping_add(1).max(1);
    }

    fn parent_ino(&self, dir_ino: u64, path: &str) -> Result<u64, Errno> {
        let trimmed = path.trim_end_matches('/');
        let (base, parent) = if let Some(rest) = trimmed.strip_prefix('/') {
            (ROOT_INO, rest.rsplit_once('/').map(|(parent, _)| parent))
        } else if dir_ino == ROOT_INO {
            (ROOT_INO, trimmed.rsplit_once('/').map(|(parent, _)| parent))
        } else {
            (dir_ino, trimmed.rsplit_once('/').map(|(parent, _)| parent))
        };
        match parent {
            Some(parent) if !parent.is_empty() => self.fs.lookup_at(base, parent),
            _ => Ok(base),
        }
    }

    fn open_ino(&mut self, path: &str, flags: OpenFlags) -> Result<(u64, bool), Errno> {
        let now = self.clock();
        let existing = if flags.nofollow() {
            self.fs.lookup_no_follow(path)
        } else {
            self.fs.lookup(path)
        };
        let (ino, created) = match existing {
            Ok(ino) => {
                if flags.create() && flags.exclusive() {
                    return Err(Errno::EEXIST);
                }
                if flags.nofollow() && self.fs.stat(ino)?.mode & S_IFMT == S_IFLNK {
                    return Err(Errno::ELOOP);
                }
                (ino, false)
            }
            Err(Errno::ENOENT) if flags.create() => {
                let mode = u16::try_from(flags.create_mode().unwrap_or(0o644))
                    .map_err(|_| Errno::EINVAL)?;
                (self.fs.create_file_at(ROOT_INO, path, mode, now)?, true)
            }
            Err(error) => return Err(error),
        };
        let stat = self.fs.stat(ino)?;
        let is_dir = stat.mode & S_IFMT == S_IFDIR;
        if flags.directory() && !is_dir {
            return Err(Errno::ENOTDIR);
        }
        if is_dir && flags.writable() {
            return Err(Errno::EISDIR);
        }
        if flags.truncate() {
            if is_dir {
                return Err(Errno::EISDIR);
            }
            self.fs.truncate(ino, 0, now)?;
        }
        Ok((ino, created))
    }

    fn status(response: &mut [u8], result: Result<(), Errno>) -> HandlerResult<usize> {
        let status = match result {
            Ok(()) => 0,
            Err(error) => -(error.0 as i32),
        };
        CallResponse::Status(status)
            .encode(response)
            .map_err(|_| Errno::EPROTO.into())
    }

    fn status_with_ino(
        response: &mut [u8],
        result: Result<u64, Errno>,
        handler: &mut RamfsHandler,
    ) -> HandlerResult<usize> {
        match result {
            Ok(ino) => {
                handler.release_if_unreferenced(ino);
                Self::status(response, Ok(()))
            }
            Err(error) => Self::status(response, Err(error)),
        }
    }
}

impl Handler for RamfsHandler {
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
        if self.opens.len() >= MAX_OPEN_FILES {
            return Err(Errno::EMFILE.into());
        }
        let (ino, created) = self.open_ino(path, flags)?;
        if created && let Ok(parent) = self.parent_ino(ROOT_INO, path) {
            self.bump_dir(parent);
        }
        self.reference(ino)?;
        if let Err(error) = self.opens.insert_with_id(
            file_id,
            OpenState {
                ino,
                dir_generation: None,
            },
            flags,
        ) {
            self.dereference(ino);
            return Err(error.into());
        }
        Ok(Opened::File(file_id))
    }

    fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        if offset != 0 {
            return Err(Errno::EINVAL.into());
        }
        let (ino, cursor, flags) = {
            let open = self.opens.get(file).ok_or(Errno::EBADF)?;
            (open.file.ino, open.cursor(), open.flags)
        };
        if !flags.readable() {
            return Err(Errno::EBADF.into());
        }
        let stat = self.fs.stat(ino)?;
        let read = if stat.mode & S_IFMT == S_IFDIR {
            let current = self.dir_generations.get(&ino).copied().unwrap_or(0);
            let recorded = {
                let open = self.opens.get_mut(file).ok_or(Errno::EBADF)?;
                match open.file.dir_generation {
                    Some(generation) => Some(generation),
                    None => {
                        open.file.dir_generation = Some(current);
                        Some(current)
                    }
                }
            };
            if recorded != Some(current) {
                return Err(Errno::EINVAL.into());
            }
            self.fs.readdir(ino, cursor, buffer, self.clock())?
        } else {
            self.fs.read(ino, cursor, buffer, self.clock())?
        };
        self.opens
            .get_mut(file)
            .ok_or(Errno::EBADF)?
            .advance(read)?;
        Ok(read)
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
        let stat = self.fs.stat(ino)?;
        if stat.mode & S_IFMT == S_IFDIR {
            return Err(Errno::EISDIR.into());
        }
        let start = if flags.append() { stat.size } else { cursor };
        let written = self.fs.write(ino, start, buffer, self.clock())?;
        self.opens
            .get_mut(file)
            .ok_or(Errno::EBADF)?
            .set_cursor(start.saturating_add(written as u64));
        Ok(written)
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        if let Some(state) = self.opens.close(file)? {
            self.dereference(state.ino);
        }
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
        let decoded = CallRequest::decode(request).map_err(|_| Errno::EPROTO)?;
        let dir_ino = self.opens.get(file).ok_or(Errno::EBADF)?.file.ino;
        let now = self.clock();
        match decoded.call {
            Call::Mkdir { mode, path } => {
                let path = text(path)?;
                let parent = self.parent_ino(dir_ino, path);
                let result = self
                    .fs
                    .mkdir_at(
                        dir_ino,
                        path,
                        u16::try_from(mode).map_err(|_| Errno::EINVAL)?,
                        now,
                    )
                    .map(|_| ());
                if result.is_ok()
                    && let Ok(parent) = parent
                {
                    self.bump_dir(parent);
                }
                Self::status(response, result)
            }
            Call::Unlink { path } => {
                let path = text(path)?;
                let parent = self.parent_ino(dir_ino, path);
                let result = self.fs.unlink_at(dir_ino, path, now);
                if result.is_ok()
                    && let Ok(parent) = parent
                {
                    self.bump_dir(parent);
                }
                Self::status_with_ino(response, result, self)
            }
            Call::Rmdir { path } => {
                let path = text(path)?;
                let parent = self.parent_ino(dir_ino, path);
                let result = self.fs.rmdir_at(dir_ino, path, now);
                if result.is_ok()
                    && let Ok(parent) = parent
                {
                    self.bump_dir(parent);
                }
                Self::status_with_ino(response, result, self)
            }
            Call::Rename { from, to } => {
                let from = text(from)?;
                let to = text(to)?;
                let from_parent = self.parent_ino(dir_ino, from);
                let to_parent = self.parent_ino(dir_ino, to);
                match self.fs.rename_at(dir_ino, from, to, now) {
                    Ok(replaced) => {
                        if let Ok(parent) = from_parent {
                            self.bump_dir(parent);
                        }
                        if let Ok(parent) = to_parent {
                            self.bump_dir(parent);
                        }
                        if let Some(ino) = replaced {
                            self.release_if_unreferenced(ino);
                        }
                        Self::status(response, Ok(()))
                    }
                    Err(error) => Self::status(response, Err(error)),
                }
            }
            Call::Truncate { size } => {
                let open = self.opens.get(file).ok_or(Errno::EBADF)?;
                let stat = self.fs.stat(open.file.ino)?;
                if stat.mode & S_IFMT == S_IFDIR {
                    return Err(Errno::EISDIR.into());
                }
                Self::status(response, self.fs.truncate(open.file.ino, size, now))
            }
            Call::Symlink { path, target } => {
                let path = text(path)?;
                let parent = self.parent_ino(dir_ino, path);
                let result = self.fs.symlink_at(dir_ino, path, target, now).map(|_| ());
                if result.is_ok()
                    && let Ok(parent) = parent
                {
                    self.bump_dir(parent);
                }
                Self::status(response, result)
            }
            Call::Readlink { path } => {
                let path = text(path)?;
                let target = self.fs.readlink_at(dir_ino, path)?;
                CallResponse::Readlink {
                    status: 0,
                    target: &target,
                }
                .encode(response)
                .map_err(|_| Errno::EPROTO.into())
            }
            Call::Chmod { mode, path } => {
                let path = text(path)?;
                Self::status(
                    response,
                    self.fs.chmod_at(
                        dir_ino,
                        path,
                        u16::try_from(mode).map_err(|_| Errno::EINVAL)?,
                        now,
                    ),
                )
            }
            Call::Sync => Self::status(response, Ok(())),
            Call::Mount { .. } | Call::Umount { .. } => Self::status(response, Err(Errno::ENOSYS)),
            Call::Statfs => {
                let statfs: StatFs = self.fs.statfs();
                CallResponse::StatFs { status: 0, statfs }
                    .encode(response)
                    .map_err(|_| Errno::EPROTO.into())
            }
        }
    }
}

fn text(bytes: &[u8]) -> Result<&str, Errno> {
    core::str::from_utf8(bytes).map_err(|_| Errno::EINVAL)
}

#[cfg(test)]
#[path = "handler_ut.rs"]
mod tests;
