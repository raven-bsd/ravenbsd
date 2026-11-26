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
    ACCESS_EXEC, ACCESS_READ, ACCESS_WRITE, Call, CallRequest, CallResponse, Credentials,
    OpenFlags, S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFSOCK, SetTime, Stat,
    StatFs, StatVfs,
};
use raven_ramfs_core::{Limits, ROOT_INO, Ramfs, SpecialKind};
use raven_scheme::{CallReply, Ctx, Handler, HandlerError, HandlerResult, OpenTable, Opened};

/// Aggregate open-file-description budget.
///
/// The caller's `RLIMIT_NOFILE` soft limit is the per-process bound; this aggregate budget must
/// not be lower than the maximum any process may set. The kernel handle table is 65,536
/// (`raven-handles::MAX_HANDLES`), the same shape as FreeBSD `maxfilesperproc`, NetBSD
/// `maxfiles` and Linux `sysctl_nr_open`; the old 256 constant refused valid opens before the
/// caller's limit.
pub const MAX_OPEN_FILES: usize = 65_536;

fn head(bytes: &[u8]) -> [u8; 8] {
    let mut prefix = [0u8; 8];
    let length = bytes.len().min(prefix.len());
    prefix[..length].copy_from_slice(&bytes[..length]);
    prefix
}

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
    trace: bool,
}

impl RamfsHandler {
    /// Create a handler over a fresh ramfs.
    pub fn new(now: fn() -> i64) -> Self {
        Self::with_trace(now, false)
    }

    /// Create a handler with an explicit root directory mode.
    pub fn with_root_mode(now: fn() -> i64, root_mode: u16) -> Self {
        Self::with_trace_root_mode(now, false, root_mode)
    }

    /// Create a handler with the optional request trace enabled.
    pub fn with_trace(now: fn() -> i64, trace: bool) -> Self {
        Self::with_trace_root_mode(now, trace, 0o755)
    }

    /// Create a handler with an explicit root mode and optional request trace.
    pub fn with_trace_root_mode(now: fn() -> i64, trace: bool, root_mode: u16) -> Self {
        Self {
            fs: Ramfs::with_root_mode(Limits::default(), now(), root_mode),
            opens: OpenTable::new(),
            refs: BTreeMap::new(),
            dir_generations: BTreeMap::new(),
            pending: None,
            now,
            trace,
        }
    }

    fn trace(&self, args: core::fmt::Arguments<'_>) {
        if !self.trace {
            return;
        }
        #[cfg(target_os = "none")]
        raven_rt::println!("{args}");
        #[cfg(not(target_os = "none"))]
        let _ = args;
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

    fn open_ino(
        &mut self,
        path: &str,
        flags: OpenFlags,
        mode: u32,
        creds: &Credentials,
    ) -> Result<(u64, bool), Errno> {
        let now = self.clock();
        let existing = if flags.nofollow() {
            self.fs.lookup_no_follow_at_checked(ROOT_INO, path, creds)
        } else {
            self.fs.lookup_at_checked(ROOT_INO, path, creds)
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
                let (_, _, parent) = self.fs.parent_and_name_checked(ROOT_INO, path, creds)?;
                if !creds.permitted(
                    (parent.mode & 0o7777) as u16,
                    parent.uid,
                    parent.gid,
                    ACCESS_WRITE | ACCESS_EXEC,
                ) {
                    return Err(Errno::EACCES);
                }
                let mode = u16::try_from(mode & 0o7777).map_err(|_| Errno::EINVAL)?;
                (
                    self.fs
                        .create_file_at_owned(ROOT_INO, path, mode, creds.euid, creds.egid, now)?,
                    true,
                )
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
        let mut want = 0;
        if flags.readable() {
            want |= ACCESS_READ;
        }
        if flags.writable() || flags.truncate() {
            want |= ACCESS_WRITE;
        }
        if want != 0 && !creds.permitted((stat.mode & 0o7777) as u16, stat.uid, stat.gid, want) {
            return Err(Errno::EACCES);
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
        mode: u32,
        ctx: &Ctx,
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
        let (ino, created) = self.open_ino(path, flags, mode, &ctx.credentials)?;
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
        self.trace(core::format_args!(
            "ramfs-trace: open file={file_id} path={path} ino={ino} req={} trace={}",
            ctx.request.0,
            ctx.trace_id
        ));
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

    fn read_with(
        &mut self,
        file: u64,
        offset: u64,
        buffer: &mut [u8],
        ctx: &Ctx,
    ) -> HandlerResult<usize> {
        if self.trace {
            self.trace(core::format_args!(
                "ramfs-trace: read file={file} off={offset} len={} ptr={:p} head={:?} req={} trace={}",
                buffer.len(),
                buffer.as_ptr(),
                head(buffer),
                ctx.request.0,
                ctx.trace_id
            ));
        }
        let result = self.read(file, offset, buffer);
        match &result {
            Ok(bytes) => self.trace(core::format_args!(
                "ramfs-trace: read-reply file={file} bytes={bytes} head={:?}",
                head(buffer)
            )),
            Err(HandlerError::Errno(error)) => self.trace(core::format_args!(
                "ramfs-trace: read-reply file={file} errno={}",
                error.description()
            )),
            Err(HandlerError::Defer(token)) => self.trace(core::format_args!(
                "ramfs-trace: read-reply file={file} defer={token}"
            )),
        }
        result
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

    fn write_with(
        &mut self,
        file: u64,
        offset: u64,
        buffer: &[u8],
        ctx: &Ctx,
    ) -> HandlerResult<usize> {
        if self.trace {
            self.trace(core::format_args!(
                "ramfs-trace: write file={file} off={offset} len={} ptr={:p} head={:?} req={} trace={}",
                buffer.len(),
                buffer.as_ptr(),
                head(buffer),
                ctx.request.0,
                ctx.trace_id
            ));
        }
        let result = self.write(file, offset, buffer);
        match &result {
            Ok(bytes) => self.trace(core::format_args!(
                "ramfs-trace: write-reply file={file} bytes={bytes} head={:?}",
                head(buffer)
            )),
            Err(HandlerError::Errno(error)) => self.trace(core::format_args!(
                "ramfs-trace: write-reply file={file} errno={}",
                error.description()
            )),
            Err(HandlerError::Defer(token)) => self.trace(core::format_args!(
                "ramfs-trace: write-reply file={file} defer={token}"
            )),
        }
        result
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
        Ok(self.fs.read(ino, offset, buffer, self.clock())?)
    }

    fn pwrite(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
        let (ino, flags) = {
            let open = self.opens.get(file).ok_or(Errno::EBADF)?;
            (open.file.ino, open.flags)
        };
        if !flags.writable() {
            return Err(Errno::EBADF.into());
        }
        let stat = self.fs.stat(ino)?;
        if stat.mode & S_IFMT == S_IFDIR {
            return Err(Errno::EISDIR.into());
        }
        Ok(self.fs.write(ino, offset, buffer, self.clock())?)
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
        let Some(new_offset) = seek_target(base, offset) else {
            return Err(Errno::EINVAL.into());
        };
        self.opens
            .get_mut(file)
            .ok_or(Errno::EBADF)?
            .set_cursor(new_offset);
        Ok(new_offset)
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
        self.call_checked(file, request, response, &Credentials::ROOT)
    }

    fn call_with(
        &mut self,
        file: u64,
        request: &[u8],
        response: &mut [u8],
        ctx: &Ctx,
    ) -> HandlerResult<CallReply> {
        let written = self.call_checked(file, request, response, &ctx.credentials)?;
        Ok(CallReply::new(written))
    }
}

impl RamfsHandler {
    fn call_checked(
        &mut self,
        file: u64,
        request: &[u8],
        response: &mut [u8],
        creds: &Credentials,
    ) -> HandlerResult<usize> {
        let decoded = CallRequest::decode(request).map_err(|_| Errno::EPROTO)?;
        let dir_ino = self.opens.get(file).ok_or(Errno::EBADF)?.file.ino;
        let now = self.clock();
        match decoded.call {
            Call::Mkdir { mode, path } => {
                let path = text(path)?;
                let (parent, _, parent_stat) =
                    self.fs.parent_and_name_checked(dir_ino, path, creds)?;
                if !creds.permitted(
                    (parent_stat.mode & 0o7777) as u16,
                    parent_stat.uid,
                    parent_stat.gid,
                    ACCESS_WRITE | ACCESS_EXEC,
                ) {
                    return Self::status(response, Err(Errno::EACCES));
                }
                let result = self
                    .fs
                    .mkdir_at_owned(
                        dir_ino,
                        path,
                        u16::try_from(mode).map_err(|_| Errno::EINVAL)?,
                        creds.euid,
                        creds.egid,
                        now,
                    )
                    .map(|_| ());
                if result.is_ok() {
                    self.bump_dir(parent);
                }
                Self::status(response, result)
            }
            Call::Unlink { path } => {
                let path = text(path)?;
                let (parent, _, parent_stat) =
                    self.fs.parent_and_name_checked(dir_ino, path, creds)?;
                if !creds.permitted(
                    (parent_stat.mode & 0o7777) as u16,
                    parent_stat.uid,
                    parent_stat.gid,
                    ACCESS_WRITE | ACCESS_EXEC,
                ) {
                    return Self::status(response, Err(Errno::EACCES));
                }
                let ino = self.fs.lookup_no_follow_at_checked(dir_ino, path, creds)?;
                let target = self.fs.stat(ino)?;
                if !creds.sticky_delete_allowed((parent_stat.mode & 0o7777) as u16, target.uid) {
                    return Self::status(response, Err(Errno::EPERM));
                }
                let result = self.fs.unlink_at(dir_ino, path, now);
                if result.is_ok() {
                    self.bump_dir(parent);
                }
                Self::status_with_ino(response, result, self)
            }
            Call::Rmdir { path } => {
                let path = text(path)?;
                let (parent, _, parent_stat) =
                    self.fs.parent_and_name_checked(dir_ino, path, creds)?;
                if !creds.permitted(
                    (parent_stat.mode & 0o7777) as u16,
                    parent_stat.uid,
                    parent_stat.gid,
                    ACCESS_WRITE | ACCESS_EXEC,
                ) {
                    return Self::status(response, Err(Errno::EACCES));
                }
                let ino = self.fs.lookup_no_follow_at_checked(dir_ino, path, creds)?;
                let target = self.fs.stat(ino)?;
                if !creds.sticky_delete_allowed((parent_stat.mode & 0o7777) as u16, target.uid) {
                    return Self::status(response, Err(Errno::EPERM));
                }
                let result = self.fs.rmdir_at(dir_ino, path, now);
                if result.is_ok() {
                    self.bump_dir(parent);
                }
                Self::status_with_ino(response, result, self)
            }
            Call::Rename { from, to } => {
                let from = text(from)?;
                let to = text(to)?;
                let (from_parent, _, from_parent_stat) =
                    self.fs.parent_and_name_checked(dir_ino, from, creds)?;
                let (to_parent, _, to_parent_stat) =
                    self.fs.parent_and_name_checked(dir_ino, to, creds)?;
                for parent_stat in [from_parent_stat, to_parent_stat] {
                    if !creds.permitted(
                        (parent_stat.mode & 0o7777) as u16,
                        parent_stat.uid,
                        parent_stat.gid,
                        ACCESS_WRITE | ACCESS_EXEC,
                    ) {
                        return Self::status(response, Err(Errno::EACCES));
                    }
                }
                let source = self.fs.lookup_no_follow_at_checked(dir_ino, from, creds)?;
                let source_stat = self.fs.stat(source)?;
                if !creds
                    .sticky_delete_allowed((from_parent_stat.mode & 0o7777) as u16, source_stat.uid)
                {
                    return Self::status(response, Err(Errno::EPERM));
                }
                if let Ok(existing) = self.fs.lookup_no_follow_at_checked(dir_ino, to, creds) {
                    let existing_stat = self.fs.stat(existing)?;
                    if !creds.sticky_delete_allowed(
                        (to_parent_stat.mode & 0o7777) as u16,
                        existing_stat.uid,
                    ) {
                        return Self::status(response, Err(Errno::EPERM));
                    }
                }
                match self.fs.rename_at(dir_ino, from, to, now) {
                    Ok(replaced) => {
                        self.bump_dir(from_parent);
                        self.bump_dir(to_parent);
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
                if !creds.permitted(
                    (stat.mode & 0o7777) as u16,
                    stat.uid,
                    stat.gid,
                    ACCESS_WRITE,
                ) {
                    return Self::status(response, Err(Errno::EACCES));
                }
                Self::status(response, self.fs.truncate(open.file.ino, size, now))
            }
            Call::Symlink { path, target } => {
                let path = text(path)?;
                let (parent, _, parent_stat) =
                    self.fs.parent_and_name_checked(dir_ino, path, creds)?;
                if !creds.permitted(
                    (parent_stat.mode & 0o7777) as u16,
                    parent_stat.uid,
                    parent_stat.gid,
                    ACCESS_WRITE | ACCESS_EXEC,
                ) {
                    return Self::status(response, Err(Errno::EACCES));
                }
                let result = self
                    .fs
                    .symlink_at_owned(dir_ino, path, target, creds.euid, creds.egid, now)
                    .map(|_| ());
                if result.is_ok() {
                    self.bump_dir(parent);
                }
                Self::status(response, result)
            }
            Call::Readlink { path } => {
                let path = text(path)?;
                let ino = self.fs.lookup_no_follow_at_checked(dir_ino, path, creds)?;
                let target = self.fs.readlink_at(dir_ino, path)?;
                let _ = ino;
                CallResponse::Readlink {
                    status: 0,
                    target: &target,
                }
                .encode(response)
                .map_err(|_| Errno::EPROTO.into())
            }
            Call::Lstat { path } => {
                let path = text(path)?;
                let ino = self.fs.lookup_no_follow_at_checked(dir_ino, path, creds)?;
                let stat = self.fs.stat(ino)?;
                CallResponse::Stat { status: 0, stat }
                    .encode(response)
                    .map_err(|_| Errno::EPROTO.into())
            }
            Call::Chmod { mode, path } => {
                let path = text(path)?;
                let ino = self.fs.lookup_at_checked(dir_ino, path, creds)?;
                let stat = self.fs.stat(ino)?;
                if !creds.is_root() && creds.euid != stat.uid {
                    return Self::status(response, Err(Errno::EPERM));
                }
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
            Call::Link { path, target } => {
                let path = text(path)?;
                let target = text(target)?;
                let (parent, _, parent_stat) =
                    self.fs.parent_and_name_checked(dir_ino, path, creds)?;
                if !creds.permitted(
                    (parent_stat.mode & 0o7777) as u16,
                    parent_stat.uid,
                    parent_stat.gid,
                    ACCESS_WRITE | ACCESS_EXEC,
                ) {
                    return Self::status(response, Err(Errno::EACCES));
                }
                if let Err(error) = self.fs.lookup_no_follow_at_checked(dir_ino, target, creds) {
                    return Self::status(response, Err(error));
                }
                let result = self.fs.link_at(dir_ino, path, target, now);
                if result.is_ok() {
                    self.bump_dir(parent);
                }
                Self::status(response, result)
            }
            Call::Chown { uid, gid, path } | Call::Lchown { uid, gid, path } => {
                let path = text(path)?;
                let follow = matches!(decoded.call, Call::Chown { .. });
                let ino = if follow {
                    self.fs.lookup_at_checked(dir_ino, path, creds)?
                } else {
                    self.fs.lookup_no_follow_at_checked(dir_ino, path, creds)?
                };
                let stat = self.fs.stat(ino)?;
                if let Err(error) = check_chown(creds, &stat, uid, gid) {
                    return Self::status(response, Err(error));
                }
                Self::status(
                    response,
                    self.fs.chown_at(dir_ino, path, uid, gid, follow, now),
                )
            }
            Call::Fchown { uid, gid } => {
                let ino = self.opens.get(file).ok_or(Errno::EBADF)?.file.ino;
                let stat = self.fs.stat(ino)?;
                if let Err(error) = check_chown(creds, &stat, uid, gid) {
                    return Self::status(response, Err(error));
                }
                Self::status(response, self.fs.chown_ino(ino, uid, gid, now))
            }
            Call::Mkfifo { mode, path } => {
                let path = text(path)?;
                let (parent, _, parent_stat) =
                    self.fs.parent_and_name_checked(dir_ino, path, creds)?;
                if !creds.permitted(
                    (parent_stat.mode & 0o7777) as u16,
                    parent_stat.uid,
                    parent_stat.gid,
                    ACCESS_WRITE | ACCESS_EXEC,
                ) {
                    return Self::status(response, Err(Errno::EACCES));
                }
                let result = self
                    .fs
                    .mkfifo_at_owned(
                        dir_ino,
                        path,
                        u16::try_from(mode).map_err(|_| Errno::EINVAL)?,
                        creds.euid,
                        creds.egid,
                        now,
                    )
                    .map(|_| ());
                if result.is_ok() {
                    self.bump_dir(parent);
                }
                Self::status(response, result)
            }
            Call::Mknod { mode, dev, path } => {
                let path = text(path)?;
                let _ = dev;
                let (parent, _, parent_stat) =
                    self.fs.parent_and_name_checked(dir_ino, path, creds)?;
                if !creds.permitted(
                    (parent_stat.mode & 0o7777) as u16,
                    parent_stat.uid,
                    parent_stat.gid,
                    ACCESS_WRITE | ACCESS_EXEC,
                ) {
                    return Self::status(response, Err(Errno::EACCES));
                }
                let kind = match mode & S_IFMT {
                    S_IFIFO => SpecialKind::Fifo,
                    S_IFCHR => SpecialKind::CharDevice,
                    S_IFBLK => SpecialKind::BlockDevice,
                    S_IFSOCK => SpecialKind::Socket,
                    _ => return Self::status(response, Err(Errno::EINVAL)),
                };
                let result = self
                    .fs
                    .mknod_at_owned(
                        dir_ino,
                        path,
                        kind,
                        u16::try_from(mode).map_err(|_| Errno::EINVAL)?,
                        creds.euid,
                        creds.egid,
                        now,
                    )
                    .map(|_| ());
                if result.is_ok() {
                    self.bump_dir(parent);
                }
                Self::status(response, result)
            }
            Call::Utimens {
                path,
                atime,
                mtime,
                follow,
            } => {
                let path = text(path)?;
                let ino = if follow {
                    self.fs.lookup_at_checked(dir_ino, path, creds)?
                } else {
                    self.fs.lookup_no_follow_at_checked(dir_ino, path, creds)?
                };
                let stat = self.fs.stat(ino)?;
                let explicit = matches!(atime, SetTime::Set(_)) || matches!(mtime, SetTime::Set(_));
                if let Err(error) = check_utimens(creds, &stat, explicit) {
                    return Self::status(response, Err(error));
                }
                let atime = match atime {
                    SetTime::Set(seconds) => Some(seconds),
                    SetTime::Now => Some(now),
                    SetTime::Omit => None,
                };
                let mtime = match mtime {
                    SetTime::Set(seconds) => Some(seconds),
                    SetTime::Now => Some(now),
                    SetTime::Omit => None,
                };
                Self::status(
                    response,
                    self.fs.utimens_at(dir_ino, path, atime, mtime, follow, now),
                )
            }
            Call::Statvfs => {
                let statvfs: StatVfs = self.fs.statfs().into();
                CallResponse::StatVfs { status: 0, statvfs }
                    .encode(response)
                    .map_err(|_| Errno::EPROTO.into())
            }
            Call::Sync => Self::status(response, Ok(())),
            Call::Access { mode, path } => {
                let path = text(path)?;
                let ino = self.fs.lookup_at_checked(dir_ino, path, creds)?;
                let stat = self.fs.stat(ino)?;
                let allowed =
                    creds.permitted((stat.mode & 0o7777) as u16, stat.uid, stat.gid, mode as u16);
                Self::status(response, if allowed { Ok(()) } else { Err(Errno::EACCES) })
            }
            Call::Mount { .. } | Call::Umount { .. } | Call::Mmap { .. } => {
                Self::status(response, Err(Errno::ENOSYS))
            }
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

fn seek_target(base: u64, offset: i64) -> Option<u64> {
    let target = i128::from(base).checked_add(i128::from(offset))?;
    if target < 0 || target > u64::MAX as i128 {
        return None;
    }
    Some(target as u64)
}

#[cfg(test)]
#[path = "handler_ut.rs"]
mod tests;
