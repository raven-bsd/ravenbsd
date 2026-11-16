// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `vfsd`: the `file:` namespace server.
//!
//! It holds the mount table and resolves every path to the owning filesystem
//! server.  An `OPEN` creates a vfsd-owned open file description that keeps the
//! target server's handle; `READ`, `WRITE`, `SEEK` and `CALL` are forwarded
//! through it.  The `/` root description additionally merges the root server's
//! entries with the names of mounts directly below `/`.
//!
//! The S25 open-attachment path is deliberately not used.  On aarch64 a handle
//! attached from `fsd-ufs` returned `EBADF` on its first `READ` after vfsd closed
//! its source handle, while the same stack passed on x86-64.  Keeping one stable
//! target handle in vfsd until `CLOSE` gives identical semantics on both
//! architectures and leaves the attachment protocol to servers that need it.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeMap;
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_fs::{
        ACCESS_EXEC, ACCESS_READ, ACCESS_WRITE, Call, CallRequest, CallResponse, Credentials,
        DIRENT_ALIGN, DIRENT_HEADER_LEN, DT_DIR, DirEntry, DirEntryHeader, DirEntryIter, MAX_PATH,
        OpenFlags, S_IFDIR, S_IFMT, Stat, WIRE_OPEN_DIRECTORY, normalize_request,
    };
    use raven_rt::entry;
    use raven_rt::println;
    use raven_scheme::{CallReply, Client, Ctx, Handler, HandlerResult, OpenTable, Opened, Server};
    use raven_vfs_core::{DEFAULT_MOUNTS, MountTable, normalized, parse_mounts};

    entry!(main);

    const READ: u32 = 1;
    const RETRIES: usize = 500;
    const RETRY_NS: u64 = 10_000_000;
    const BUFFER: usize = 4096;
    /// Budget for proxy open-file descriptions.
    const MAX_OPEN_FILES: usize = 256;

    struct Proxy {
        target: Handle,
        path: String,
        root_listing: bool,
        merged: Option<Vec<u8>>,
        mount_generation: u32,
    }

    struct VfsHandler {
        table: MountTable,
        opens: OpenTable<Proxy>,
        mount_generation: u32,
    }

    impl VfsHandler {
        fn new(table: MountTable) -> Self {
            Self {
                table,
                opens: OpenTable::new(),
                mount_generation: 1,
            }
        }

        fn bump_mounts(&mut self) {
            self.mount_generation = self.mount_generation.wrapping_add(1).max(1);
        }

        fn absolute(&self, base: &str, path: &str) -> Result<String, Errno> {
            let joined = if path.starts_with('/') {
                path.to_string()
            } else if base == "/" {
                format!("/{path}")
            } else {
                format!("{base}/{path}")
            };
            normalized(&joined)
        }

        fn normalize_request_path(&self, path: &str) -> Result<(String, bool), Errno> {
            let absolute = if path.is_empty() {
                "/".to_string()
            } else if path.starts_with('/') {
                path.to_string()
            } else {
                format!("/{path}")
            };
            let mut buffer = [0u8; MAX_PATH];
            let normalized =
                normalize_request(&absolute, &mut buffer).map_err(|error| error.errno())?;
            Ok((normalized.path.to_string(), normalized.requires_dir))
        }

        fn target_stat(&self, scheme: &str, suffix: &str) -> Result<Stat, Errno> {
            let client = Client::new();
            let path = format!("{scheme}{suffix}");
            let handle = client.open(&path, READ)?;
            let stat = client.fstat(handle);
            let _ = client.close(handle);
            stat
        }

        fn check_traverse(
            &self,
            scheme: &str,
            suffix: &str,
            creds: &Credentials,
        ) -> Result<(), Errno> {
            let mut prefix = String::new();
            for component in suffix.split('/').filter(|component| !component.is_empty()) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(component);
                let stat = self.target_stat(scheme, &prefix)?;
                if stat.mode & S_IFMT == S_IFDIR
                    && !creds.permitted(
                        (stat.mode & 0o7777) as u16,
                        stat.uid,
                        stat.gid,
                        ACCESS_EXEC,
                    )
                {
                    return Err(Errno::EACCES);
                }
            }
            Ok(())
        }

        fn check_parent(
            &self,
            scheme: &str,
            suffix: &str,
            creds: &Credentials,
        ) -> Result<(), Errno> {
            let parent = parent_suffix(suffix);
            self.check_traverse(scheme, parent, creds)?;
            let stat = self.target_stat(scheme, parent)?;
            if stat.mode & S_IFMT != S_IFDIR {
                return Err(Errno::ENOTDIR);
            }
            if !creds.permitted(
                (stat.mode & 0o7777) as u16,
                stat.uid,
                stat.gid,
                ACCESS_WRITE | ACCESS_EXEC,
            ) {
                return Err(Errno::EACCES);
            }
            Ok(())
        }

        fn check_open(
            &self,
            scheme: &str,
            suffix: &str,
            creds: &Credentials,
            flags: OpenFlags,
        ) -> Result<(), Errno> {
            if flags.create() {
                self.check_parent(scheme, suffix, creds)?;
                match self.target_stat(scheme, suffix) {
                    Ok(stat) => return check_stat_access(creds, stat, flags),
                    Err(Errno::ENOENT) => return Ok(()),
                    Err(error) => return Err(error),
                }
            }
            self.check_traverse(scheme, suffix, creds)?;
            let stat = self.target_stat(scheme, suffix)?;
            check_stat_access(creds, stat, flags)
        }

        fn build_root_listing(&self, target: Handle) -> Result<Vec<u8>, Errno> {
            let client = Client::new();
            let mut entries: BTreeMap<String, (u64, u8)> = BTreeMap::new();
            let mut buffer = [0u8; BUFFER];
            loop {
                let length = client.read(target, 0, &mut buffer)?;
                if length == 0 {
                    break;
                }
                for entry in DirEntryIter::new(&buffer[..length]) {
                    let entry = entry.map_err(|_| Errno::EPROTO)?;
                    let name = String::from_utf8(entry.name.to_vec()).map_err(|_| Errno::EINVAL)?;
                    entries
                        .entry(name)
                        .or_insert((entry.header.ino, entry.header.kind));
                }
            }
            for mount in self.table.mounts_below("/") {
                let name = mount.path.strip_prefix('/').unwrap_or(&mount.path);
                entries.entry(name.to_string()).or_insert((0, DT_DIR));
            }
            let mut packed = Vec::new();
            for (name, (ino, kind)) in entries {
                let entry = DirEntry {
                    header: DirEntryHeader {
                        ino,
                        kind,
                        name_len: u8::try_from(name.len()).map_err(|_| Errno::ENAMETOOLONG)?,
                        pad: [0; 6],
                    },
                    name: name.as_bytes(),
                };
                let length = entry.encoded_len();
                let offset = packed.len();
                packed.resize(offset + length, 0);
                entry
                    .encode(&mut packed[offset..])
                    .map_err(|_| Errno::EPROTO)?;
            }
            Ok(packed)
        }

        fn ensure_root(&mut self, file: u64) -> Result<(), Errno> {
            let current_generation = self.mount_generation;
            let (target, needs_build) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (
                    entry.file.target,
                    entry.file.merged.is_none()
                        || entry.file.mount_generation != current_generation,
                )
            };
            if needs_build {
                let packed = self.build_root_listing(target)?;
                let entry = self.opens.get_mut(file).ok_or(Errno::EBADF)?;
                entry.file.merged = Some(packed);
                entry.file.mount_generation = current_generation;
                entry.set_cursor(0);
            }
            Ok(())
        }

        fn root_explicit(
            &mut self,
            file: u64,
            offset: u64,
            buffer: &mut [u8],
        ) -> Result<usize, Errno> {
            self.ensure_root(file)?;
            let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
            let packed = entry.file.merged.as_ref().ok_or(Errno::EIO)?;
            let start = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
            let available = packed.get(start..).unwrap_or(&[]);
            let length = available.len().min(buffer.len());
            buffer
                .get_mut(..length)
                .ok_or(Errno::EINVAL)?
                .copy_from_slice(available.get(..length).ok_or(Errno::EINVAL)?);
            Ok(length)
        }

        fn read_root(&mut self, file: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
            self.ensure_root(file)?;
            let (written, next) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                let packed = entry.file.merged.as_ref().ok_or(Errno::EIO)?;
                let mut index = usize::try_from(entry.cursor()).map_err(|_| Errno::EOVERFLOW)?;
                let mut written = 0usize;
                while index < packed.len() {
                    let header = packed.get(index..index + 16).ok_or(Errno::EPROTO)?;
                    let name_len = usize::from(header[9]);
                    let length =
                        (DIRENT_HEADER_LEN + name_len + DIRENT_ALIGN - 1) & !(DIRENT_ALIGN - 1);
                    let record = packed.get(index..index + length).ok_or(Errno::EPROTO)?;
                    if length > buffer.len().saturating_sub(written) {
                        if written == 0 {
                            return Err(Errno::EINVAL);
                        }
                        break;
                    }
                    buffer[written..written + length].copy_from_slice(record);
                    written += length;
                    index += length;
                }
                (written, index)
            };
            self.opens
                .get_mut(file)
                .ok_or(Errno::EBADF)?
                .set_cursor(next as u64);
            Ok(written)
        }

        fn forward_one(
            &self,
            call: Call<'_>,
            base: &str,
            response: &mut [u8],
            creds: &Credentials,
        ) -> Result<usize, Errno> {
            let raw = single_path(&call).ok_or(Errno::EINVAL)?;
            let path = self.absolute(base, &bytes_to_string(raw)?)?;
            let (mount_id, suffix) = self.table.resolve(&path)?;
            let scheme = self
                .table
                .get(mount_id)
                .ok_or(Errno::ENOENT)?
                .scheme
                .clone();
            match call {
                Call::Mkdir { .. } | Call::Symlink { .. } => {
                    self.check_parent(&scheme, suffix, creds)?;
                }
                Call::Unlink { .. } | Call::Rmdir { .. } => {
                    self.check_parent(&scheme, suffix, creds)?;
                    let target = self.target_stat(&scheme, suffix)?;
                    let parent = self.target_stat(&scheme, parent_suffix(suffix))?;
                    if !creds.sticky_delete_allowed((parent.mode & 0o7777) as u16, target.uid) {
                        return Err(Errno::EPERM);
                    }
                }
                Call::Readlink { .. } => {
                    self.check_traverse(&scheme, parent_suffix(suffix), creds)?;
                }
                Call::Chmod { .. } => {
                    self.check_traverse(&scheme, parent_suffix(suffix), creds)?;
                    let target = self.target_stat(&scheme, suffix)?;
                    if !creds.is_root() && creds.euid != target.uid {
                        return Err(Errno::EPERM);
                    }
                }
                _ => {}
            }
            let client = Client::new();
            let root = client.open(&scheme, READ)?;
            let request = CallRequest::new(rebuild_single(&call, suffix.as_bytes())?);
            let result = proxy_call(&client, root, &request, response);
            let _ = client.close(root);
            result
        }

        fn forward_rename(
            &self,
            from: &str,
            to: &str,
            response: &mut [u8],
            creds: &Credentials,
        ) -> Result<usize, Errno> {
            self.table.rename(from, to)?;
            let (mount_id, suffix) = self.table.resolve(from)?;
            let (_, to_suffix) = self.table.resolve(to)?;
            let scheme = self
                .table
                .get(mount_id)
                .ok_or(Errno::ENOENT)?
                .scheme
                .clone();
            if self.table.resolve(from)?.0 != self.table.resolve(to)?.0 {
                return Err(Errno::EXDEV);
            }
            self.check_parent(&scheme, suffix, creds)?;
            self.check_parent(&scheme, to_suffix, creds)?;
            let source = self.target_stat(&scheme, suffix)?;
            let source_parent = self.target_stat(&scheme, parent_suffix(suffix))?;
            if !creds.sticky_delete_allowed((source_parent.mode & 0o7777) as u16, source.uid) {
                return Err(Errno::EPERM);
            }
            if let Ok(dest) = self.target_stat(&scheme, to_suffix) {
                let dest_parent = self.target_stat(&scheme, parent_suffix(to_suffix))?;
                if !creds.sticky_delete_allowed((dest_parent.mode & 0o7777) as u16, dest.uid) {
                    return Err(Errno::EPERM);
                }
            }
            let client = Client::new();
            let root = client.open(&scheme, READ)?;
            let request = CallRequest::new(Call::Rename {
                from: suffix.as_bytes(),
                to: to_suffix.as_bytes(),
            });
            let result = proxy_call(&client, root, &request, response);
            let _ = client.close(root);
            result
        }
    }

    fn parent_suffix(suffix: &str) -> &str {
        let trimmed = suffix.trim_end_matches('/');
        match trimmed.rsplit_once('/') {
            Some((parent, _)) => parent,
            None => "",
        }
    }

    fn check_stat_access(creds: &Credentials, stat: Stat, flags: OpenFlags) -> Result<(), Errno> {
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
        if creds.permitted((stat.mode & 0o7777) as u16, stat.uid, stat.gid, want) {
            Ok(())
        } else {
            Err(Errno::EACCES)
        }
    }

    fn single_path<'a>(call: &Call<'a>) -> Option<&'a [u8]> {
        match call {
            Call::Mkdir { path, .. }
            | Call::Unlink { path }
            | Call::Rmdir { path }
            | Call::Readlink { path }
            | Call::Chmod { path, .. }
            | Call::Symlink { path, .. } => Some(*path),
            _ => None,
        }
    }

    fn bytes_to_string(bytes: &[u8]) -> Result<String, Errno> {
        core::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| Errno::EINVAL)
    }

    /// Forward a read through a local buffer.
    ///
    /// The client buffer is a kernel capture mapped into `vfsd`, and the kernel
    /// cannot capture that window again for the target server: `capture::create`
    /// looks the source up in the VMA tree and returns `EFAULT`.  Stage the data
    /// through a stack buffer owned by `vfsd` instead.  `offset` selects a
    /// positional read; `None` advances the target's cursor.
    fn copy_read(
        client: &Client,
        target: Handle,
        offset: Option<u64>,
        buffer: &mut [u8],
    ) -> Result<usize, Errno> {
        let mut scratch = [0u8; BUFFER];
        let mut done = 0usize;
        while done < buffer.len() {
            let capacity = (buffer.len() - done).min(scratch.len());
            let read = match offset {
                Some(base) => {
                    let position = base.checked_add(done as u64).ok_or(Errno::EOVERFLOW)?;
                    client.pread(target, position, &mut scratch[..capacity])?
                }
                None => client.read(target, 0, &mut scratch[..capacity])?,
            };
            if read > capacity {
                return Err(Errno::EPROTO);
            }
            buffer[done..done + read].copy_from_slice(&scratch[..read]);
            done += read;
            if read < capacity {
                break;
            }
        }
        Ok(done)
    }

    /// Forward a write through a local buffer; see [`copy_read`].
    fn copy_write(
        client: &Client,
        target: Handle,
        offset: Option<u64>,
        buffer: &[u8],
    ) -> Result<usize, Errno> {
        let mut scratch = [0u8; BUFFER];
        let mut done = 0usize;
        while done < buffer.len() {
            let capacity = (buffer.len() - done).min(scratch.len());
            scratch[..capacity].copy_from_slice(&buffer[done..done + capacity]);
            let written = match offset {
                Some(base) => {
                    let position = base.checked_add(done as u64).ok_or(Errno::EOVERFLOW)?;
                    client.pwrite(target, position, &scratch[..capacity])?
                }
                None => client.write(target, 0, &scratch[..capacity])?,
            };
            if written > capacity {
                return Err(Errno::EPROTO);
            }
            done += written;
            if written < capacity {
                break;
            }
        }
        Ok(done)
    }

    /// Forward a small control reply through a local buffer.
    ///
    /// A server capture (`vfsd`'s own response buffer) cannot be captured again
    /// for the target server, so the target writes into this stack scratch and
    /// `vfsd` copies the bytes into the waiting client buffer.
    fn proxy_call(
        client: &Client,
        target: Handle,
        request: &CallRequest<'_>,
        response: &mut [u8],
    ) -> Result<usize, Errno> {
        let mut scratch = [0u8; MAX_PATH + 64];
        let capacity = response.len().min(scratch.len());
        let written = client.call_typed(target, request, &mut scratch[..capacity])?;
        if written > response.len() {
            return Err(Errno::E2BIG);
        }
        response[..written].copy_from_slice(&scratch[..written]);
        Ok(written)
    }

    fn rebuild_single<'a>(call: &Call<'a>, path: &'a [u8]) -> Result<Call<'a>, Errno> {
        Ok(match call {
            Call::Mkdir { mode, .. } => Call::Mkdir { mode: *mode, path },
            Call::Unlink { .. } => Call::Unlink { path },
            Call::Rmdir { .. } => Call::Rmdir { path },
            Call::Readlink { .. } => Call::Readlink { path },
            Call::Chmod { mode, .. } => Call::Chmod { mode: *mode, path },
            Call::Symlink { target, .. } => Call::Symlink { path, target },
            _ => return Err(Errno::EINVAL),
        })
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

    impl Handler for VfsHandler {
        fn open_with(
            &mut self,
            path: &str,
            flags: OpenFlags,
            _mode: u32,
            ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let (path, requires_dir) = self.normalize_request_path(path)?;
            let directory = requires_dir || flags.directory();
            let target_flags = OpenFlags::from_wire(flags.to_wire() & !WIRE_OPEN_DIRECTORY)
                .map_err(|_| Errno::EINVAL)?;
            let (mount_id, suffix) = self.table.resolve(&path)?;
            let scheme = self
                .table
                .get(mount_id)
                .ok_or(Errno::ENOENT)?
                .scheme
                .clone();
            let target_path = format!("{scheme}{suffix}");
            self.check_open(&scheme, suffix, &ctx.credentials, target_flags)?;
            let client = Client::new();
            let target = client.open_flags(&target_path, target_flags)?;
            if directory || path == "/" {
                let stat = match client.fstat(target) {
                    Ok(stat) => stat,
                    Err(error) => {
                        let _ = client.close(target);
                        return Err(error.into());
                    }
                };
                if stat.mode & S_IFMT != S_IFDIR {
                    let _ = client.close(target);
                    return Err(Errno::ENOTDIR.into());
                }
            }
            if self.opens.len() >= MAX_OPEN_FILES {
                let _ = client.close(target);
                return Err(Errno::EMFILE.into());
            }
            let id = self.opens.insert(
                Proxy {
                    target,
                    root_listing: path == "/",
                    path,
                    merged: None,
                    mount_generation: self.mount_generation,
                },
                flags,
            )?;
            Ok(Opened::File(id))
        }

        fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            if offset != 0 {
                return Err(Errno::EINVAL.into());
            }
            let (target, root) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (entry.file.target, entry.file.root_listing)
            };
            if root {
                Ok(self.read_root(file, buffer)?)
            } else {
                Ok(copy_read(&Client::new(), target, None, buffer)?)
            }
        }

        fn write(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            if offset != 0 {
                return Err(Errno::EINVAL.into());
            }
            let target = self.opens.get(file).ok_or(Errno::EBADF)?.file.target;
            Ok(copy_write(&Client::new(), target, None, buffer)?)
        }

        fn pread(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            let (target, root) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (entry.file.target, entry.file.root_listing)
            };
            if root {
                Ok(self.root_explicit(file, offset, buffer)?)
            } else {
                Ok(copy_read(&Client::new(), target, Some(offset), buffer)?)
            }
        }

        fn pwrite(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            let (target, root) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (entry.file.target, entry.file.root_listing)
            };
            if root {
                return Err(Errno::EISDIR.into());
            }
            Ok(copy_write(&Client::new(), target, Some(offset), buffer)?)
        }

        fn seek(&mut self, file: u64, offset: i64, whence: u32) -> HandlerResult<u64> {
            let (target, root) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (entry.file.target, entry.file.root_listing)
            };
            if !root {
                return Ok(Client::new().seek(target, offset, whence)?);
            }
            self.ensure_root(file)?;
            let (cursor, size) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (
                    entry.cursor(),
                    entry
                        .file
                        .merged
                        .as_ref()
                        .map_or(0, |packed| packed.len() as u64),
                )
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
            let new_offset = target as u64;
            self.opens
                .get_mut(file)
                .ok_or(Errno::EBADF)?
                .set_cursor(new_offset);
            Ok(new_offset)
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            if let Some(proxy) = self.opens.close(file)? {
                let _ = Client::new().close(proxy.target);
            }
            Ok(())
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            Ok(self.opens.dup(file)?)
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let target = self.opens.get(file).ok_or(Errno::EBADF)?.file.target;
            Ok(Client::new().fstat(target)?)
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

    impl VfsHandler {
        fn call_checked(
            &mut self,
            file: u64,
            request: &[u8],
            response: &mut [u8],
            creds: &Credentials,
        ) -> HandlerResult<usize> {
            let (target, base) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (entry.file.target, entry.file.path.clone())
            };
            let decoded = CallRequest::decode(request).map_err(|_| Errno::EPROTO)?;
            match decoded.call {
                Call::Mount { path, scheme } => {
                    if !creds.is_root() {
                        return Err(Errno::EPERM.into());
                    }
                    let path = bytes_to_string(path)?;
                    let path = normalized(&path)?;
                    let scheme = bytes_to_string(scheme)?;
                    self.table.mount(&path, &scheme)?;
                    self.bump_mounts();
                    status(response, Ok(()))
                }
                Call::Umount { path } => {
                    if !creds.is_root() {
                        return Err(Errno::EPERM.into());
                    }
                    let path = bytes_to_string(path)?;
                    let path = normalized(&path)?;
                    self.table.umount(&path, 0)?;
                    self.bump_mounts();
                    status(response, Ok(()))
                }
                Call::Sync => {
                    let client = Client::new();
                    for mount in self.table.mounts() {
                        if let Ok(root) = client.open(&mount.scheme, READ) {
                            let mut scratch = [0u8; 64];
                            match client.call_typed(
                                root,
                                &CallRequest::new(Call::Sync),
                                &mut scratch,
                            ) {
                                Ok(_) | Err(Errno::EROFS) | Err(Errno::ENOSYS) => {}
                                Err(error) => return Err(error.into()),
                            }
                            let _ = client.close(root);
                        }
                    }
                    status(response, Ok(()))
                }
                Call::Statfs => {
                    let (mount_id, _) = self.table.resolve("/")?;
                    let scheme = self
                        .table
                        .get(mount_id)
                        .ok_or(Errno::ENOENT)?
                        .scheme
                        .clone();
                    let client = Client::new();
                    let root = client.open(&scheme, READ)?;
                    let result = proxy_call(&client, root, &decoded, response);
                    let _ = client.close(root);
                    Ok(result?)
                }
                Call::Truncate { .. } => {
                    let stat = Client::new().fstat(target)?;
                    if !creds.permitted(
                        (stat.mode & 0o7777) as u16,
                        stat.uid,
                        stat.gid,
                        ACCESS_WRITE,
                    ) {
                        return Err(Errno::EACCES.into());
                    }
                    Ok(proxy_call(&Client::new(), target, &decoded, response)?)
                }
                Call::Rename { from, to } => {
                    let from = self.absolute(&base, &bytes_to_string(from)?)?;
                    let to = self.absolute(&base, &bytes_to_string(to)?)?;
                    Ok(self.forward_rename(&from, &to, response, creds)?)
                }
                Call::Access { mode, path } => {
                    let path = self.absolute(&base, &bytes_to_string(path)?)?;
                    let (mount_id, suffix) = self.table.resolve(&path)?;
                    let scheme = self
                        .table
                        .get(mount_id)
                        .ok_or(Errno::ENOENT)?
                        .scheme
                        .clone();
                    self.check_traverse(&scheme, suffix, creds)?;
                    let stat = self.target_stat(&scheme, suffix)?;
                    let allowed = creds.permitted(
                        (stat.mode & 0o7777) as u16,
                        stat.uid,
                        stat.gid,
                        mode as u16,
                    );
                    status(response, if allowed { Ok(()) } else { Err(Errno::EACCES) })
                }
                Call::Mkdir { .. }
                | Call::Unlink { .. }
                | Call::Rmdir { .. }
                | Call::Symlink { .. }
                | Call::Readlink { .. }
                | Call::Chmod { .. } => {
                    Ok(self.forward_one(decoded.call, &base, response, creds)?)
                }
            }
        }
    }

    fn wait_scheme(scheme: &str) -> Result<(), Errno> {
        let client = Client::new();
        let mut attempt = 0usize;
        while attempt < RETRIES {
            match client.open(scheme, READ) {
                Ok(handle) => {
                    let _ = client.close(handle);
                    return Ok(());
                }
                Err(Errno::ENOENT) => {
                    let _ = raven_rt::nanosleep(RETRY_NS);
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
        Err(Errno::ETIMEDOUT)
    }

    fn load_mounts() -> Result<Vec<(String, String)>, Errno> {
        match raven_rt::fs::read_to_vec("initfs:/etc/mounts") {
            Ok(data) => {
                let text = core::str::from_utf8(&data).map_err(|_| Errno::EINVAL)?;
                parse_mounts(text)
            }
            Err(Errno::ENOENT) => Ok(DEFAULT_MOUNTS
                .iter()
                .map(|(path, scheme)| (path.to_string(), scheme.to_string()))
                .collect()),
            Err(error) => Err(error),
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let mounts = match load_mounts() {
            Ok(mounts) => mounts,
            Err(_) => return 1,
        };
        let mut table = MountTable::new();
        for (path, scheme) in mounts {
            if let Err(error) = wait_scheme(&scheme) {
                if path == "/" {
                    println!("vfsd: root scheme {scheme} unavailable");
                    return 2;
                }
                println!("vfsd: skipped {path} on {scheme}: {}", error.description());
                continue;
            }
            match table.mount(&path, &scheme) {
                Ok(_) => println!("vfsd: mounted {path} on {scheme}"),
                Err(error) => println!(
                    "vfsd: cannot mount {path} on {scheme}: {}",
                    error.description()
                ),
            }
        }
        if table.resolve("/").is_err() {
            println!("vfsd: no root mount");
            return 3;
        }
        let mut server = match Server::register("file") {
            Ok(server) => server,
            Err(error) => {
                println!("vfsd: register: {}", error.description());
                return 4;
            }
        };
        let mut handler = VfsHandler::new(table);
        server.run(&mut handler)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
