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
mod trace;

mod chunk;
mod pivot;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeMap;
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, Rights, prot};
    use raven_pager::{PAGE_IN, PAGE_OUT, PagerPacket};
    use raven_proto_fs::{
        ACCESS_EXEC, ACCESS_READ, ACCESS_WRITE, Call, CallRequest, CallResponse, Credentials,
        DIRENT_ALIGN, DIRENT_HEADER_LEN, DT_DIR, DirEntry, DirEntryHeader, DirEntryIter, MAX_PATH,
        OpenFlags, S_IFDIR, S_IFLNK, S_IFMT, Stat, StatFs, WIRE_OPEN_DIRECTORY, normalize_request,
    };
    use raven_rt::entry;
    use raven_rt::println;
    use raven_scheme::{
        CallReply, Client, Ctx, Handler, HandlerError, HandlerResult, OpenTable, Opened, Server,
    };
    use raven_vfs_core::{DEFAULT_MOUNTS, MountTable, normalized, parse_mounts};

    entry!(main);

    const READ: u32 = 1;
    const RETRIES: usize = 500;
    const RETRY_NS: u64 = 10_000_000;
    /// Directory-page staging buffer; directory entries are small and stay at one page.
    const DIR_BUFFER: usize = 4096;
    /// Proxy read/write chunk. The client read size is the upper bound; a 64 KiB chunk removes
    /// the 4 KiB nested-call storm for large file reads without a 64 KiB stack array.
    const PROXY_CHUNK: usize = 64 * 1024;
    const PAGE: usize = 4096;
    /// Budget for proxy open-file descriptions.
    ///
    /// The caller's `RLIMIT_NOFILE` soft limit is the per-process bound; this aggregate budget must
    /// not be lower than the maximum any process may set. The kernel handle table is 65,536
    /// (`raven-handles::MAX_HANDLES`), the same shape as FreeBSD `maxfilesperproc`, NetBSD
    /// `maxfiles` and Linux `sysctl_nr_open`; the old 256 constant refused valid opens before the
    /// caller's limit.
    const MAX_OPEN_FILES: usize = 65_536;

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    /// S11 `Opcode::Read` wire value.
    const READ_OPCODE: u16 = 2;

    fn head(bytes: &[u8]) -> [u8; 8] {
        let mut prefix = [0u8; 8];
        let length = bytes.len().min(prefix.len());
        prefix[..length].copy_from_slice(&bytes[..length]);
        prefix
    }

    struct PagerRegion {
        /// Mount that owns the file; together with `ino` this is the file identity.
        mount_id: u32,
        ino: u64,
        /// A private duplicate of the open target. It keeps the backing file readable for the
        /// pager after the client closes the descriptor that created the mapping.
        target: Handle,
        offset: u64,
        len: u64,
        shared: bool,
        object: Handle,
        /// Open descriptions that mapped this object. The object is released only when the last
        /// one closes, so a second mapping keeps its page source alive.
        owners: Vec<u64>,
    }

    struct Proxy {
        target: Handle,
        /// Mount-table id captured at open time; the pager identity uses it with the inode.
        mount_id: u32,
        path: String,
        /// The caller's process root captured at open time; path CALLs must stay under it.
        root: String,
        root_listing: bool,
        merged: Option<Vec<u8>>,
        mount_generation: u32,
    }

    #[derive(Clone, Copy)]
    struct DiagEntry {
        op: u8,
        file: u64,
        offset: u64,
        len: u64,
        request: u32,
        accepted: bool,
    }

    struct VfsHandler {
        table: MountTable,
        opens: OpenTable<Proxy>,
        pagers: BTreeMap<u32, PagerRegion>,
        mount_generation: u32,
        probe: crate::trace::Probe,
        trace: bool,
        diag: [Option<DiagEntry>; 16],
        diag_len: usize,
    }

    impl VfsHandler {
        fn new(table: MountTable, trace: bool) -> Self {
            Self {
                table,
                opens: OpenTable::new(),
                pagers: BTreeMap::new(),
                mount_generation: 1,
                probe: crate::trace::Probe::server(),
                trace,
                diag: [None; 16],
                diag_len: 0,
            }
        }

        fn note_diag(
            &mut self,
            op: u8,
            file: u64,
            offset: u64,
            len: u64,
            request: u32,
            accepted: bool,
        ) {
            let index = self.diag_len % 16;
            if let Some(slot) = self.diag.get_mut(index) {
                *slot = Some(DiagEntry {
                    op,
                    file,
                    offset,
                    len,
                    request,
                    accepted,
                });
                self.diag_len = self.diag_len.saturating_add(1);
            }
        }

        fn dump_diag(&self) {
            println!("vfsd-diag: begin entries={}", self.diag_len.min(16));
            let start = self.diag_len.saturating_sub(16);
            for index in start..self.diag_len {
                if let Some(entry) = self.diag.get(index % 16).copied().flatten() {
                    println!(
                        "vfsd-diag: op={} file={} off={} len={} req={} accepted={}",
                        entry.op,
                        entry.file,
                        entry.offset,
                        entry.len,
                        entry.request,
                        entry.accepted
                    );
                }
            }
            println!("vfsd-diag: end");
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

        fn target_lstat(&self, scheme: &str, suffix: &str) -> Result<Stat, Errno> {
            let client = Client::new();
            let root = client.open(scheme, READ)?;
            let request = CallRequest::new(Call::Lstat {
                path: suffix.as_bytes(),
            });
            let mut response = [0u8; 128];
            let result = client.call_typed(root, &request, &mut response);
            let _ = client.close(root);
            let length = result?;
            match CallResponse::decode(request.op(), &response[..length]) {
                Ok(CallResponse::Stat { status: 0, stat }) => Ok(stat),
                Ok(CallResponse::Stat { status, .. })
                | Ok(CallResponse::Status(status))
                | Ok(CallResponse::Readlink { status, .. })
                | Ok(CallResponse::StatFs { status, .. })
                | Ok(CallResponse::StatVfs { status, .. }) => Err(errno_from_status(status)),
                Err(_) => Err(Errno::EPROTO),
            }
        }

        fn target_readlink(&self, scheme: &str, suffix: &str) -> Result<String, Errno> {
            let client = Client::new();
            let root = client.open(scheme, READ)?;
            let request = CallRequest::new(Call::Readlink {
                path: suffix.as_bytes(),
            });
            let mut response = [0u8; MAX_PATH + 8];
            let result = client.call_typed(root, &request, &mut response);
            let _ = client.close(root);
            let length = result?;
            match CallResponse::decode(request.op(), &response[..length]) {
                Ok(CallResponse::Readlink { status: 0, target }) => core::str::from_utf8(target)
                    .map(String::from)
                    .map_err(|_| Errno::EINVAL),
                Ok(CallResponse::Status(status))
                | Ok(CallResponse::Stat { status, .. })
                | Ok(CallResponse::Readlink { status, .. })
                | Ok(CallResponse::StatFs { status, .. })
                | Ok(CallResponse::StatVfs { status, .. }) => Err(errno_from_status(status)),
                Err(_) => Err(Errno::EPROTO),
            }
        }

        fn normalize_under_root(
            &self,
            root: &str,
            base: &str,
            target: &str,
        ) -> Result<String, Errno> {
            if root == "/" {
                return Ok(target.to_string());
            }
            let combined = if target.starts_with('/') {
                format!("{root}{target}")
            } else if base == "/" {
                format!("/{target}")
            } else {
                format!("{base}/{target}")
            };
            if combined != root && !combined.starts_with(&format!("{root}/")) {
                return Err(Errno::ENOENT);
            }
            let root_parts: Vec<&str> = root.split('/').filter(|part| !part.is_empty()).collect();
            let mut parts: Vec<String> = root_parts.iter().map(|part| part.to_string()).collect();
            let rest = combined.strip_prefix(root).unwrap_or("");
            for part in rest.split('/') {
                match part {
                    "" | "." => {}
                    ".." => {
                        if parts.len() > root_parts.len() {
                            parts.pop();
                        }
                    }
                    _ => parts.push(part.to_string()),
                }
            }
            Ok(format!("/{}", parts.join("/")))
        }

        /// Resolve every symlink in `path` while keeping the result below `root`.
        fn resolve_rooted(
            &self,
            path: &str,
            root: &str,
            follow_final: bool,
        ) -> Result<String, Errno> {
            if root == "/" {
                return Ok(path.to_string());
            }
            if path != root && !path.starts_with(&format!("{root}/")) {
                return Err(Errno::ENOENT);
            }
            let mut current = root.to_string();
            let mut components: Vec<String> = path
                .strip_prefix(root)
                .unwrap_or("")
                .split('/')
                .filter(|part| !part.is_empty())
                .map(String::from)
                .collect();
            let mut index = 0usize;
            let mut links = 0usize;
            while index < components.len() {
                let component = components[index].clone();
                if component == "." {
                    index += 1;
                    continue;
                }
                if component == ".." {
                    if current != root {
                        current = current
                            .rsplit_once('/')
                            .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
                            .unwrap_or("/")
                            .to_string();
                    }
                    index += 1;
                    continue;
                }
                let candidate = if current == "/" {
                    format!("/{component}")
                } else {
                    format!("{current}/{component}")
                };
                let (mount_id, suffix) = self.table.resolve(&candidate)?;
                let scheme = self
                    .table
                    .get(mount_id)
                    .ok_or(Errno::ENOENT)?
                    .scheme
                    .clone();
                match self.target_lstat(&scheme, suffix) {
                    Ok(stat) if stat.mode & S_IFMT == S_IFLNK => {
                        if !follow_final && index + 1 == components.len() {
                            return Err(Errno::ELOOP);
                        }
                        links += 1;
                        if links > 40 {
                            return Err(Errno::ELOOP);
                        }
                        let target = self.target_readlink(&scheme, suffix)?;
                        if target.is_empty() {
                            return Err(Errno::ENOENT);
                        }
                        let base = if target.starts_with('/') {
                            root.to_string()
                        } else {
                            candidate
                                .rsplit_once('/')
                                .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
                                .unwrap_or("/")
                                .to_string()
                        };
                        let resolved = self.normalize_under_root(root, &base, &target)?;
                        components = resolved
                            .strip_prefix(root)
                            .unwrap_or("")
                            .split('/')
                            .filter(|part| !part.is_empty())
                            .map(String::from)
                            .collect();
                        index = 0;
                        current = root.to_string();
                    }
                    Ok(stat) => {
                        if index + 1 < components.len() && stat.mode & S_IFMT != S_IFDIR {
                            return Err(Errno::ENOTDIR);
                        }
                        current = candidate;
                        index += 1;
                    }
                    Err(Errno::ENOENT) if index + 1 == components.len() => {
                        return Ok(candidate);
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(current)
        }

        fn mmap_call(
            &mut self,
            file: u64,
            offset: u64,
            len: u64,
            shared: bool,
        ) -> HandlerResult<CallReply> {
            self.note_diag(4, file, offset, len, 0, true);
            if self.trace {
                println!(
                    "vfsd-trace: mmap file={file} offset={offset} len={len} shared={shared} open={}",
                    self.opens.get(file).is_some()
                );
            }
            let result = self.mmap_call_inner(file, offset, len, shared);
            if self.trace {
                match &result {
                    Ok(_) => println!("vfsd-trace: mmap-reply file={file} ok"),
                    Err(HandlerError::Errno(error)) => println!(
                        "vfsd-trace: mmap-reply file={file} errno={}",
                        error.description()
                    ),
                    Err(HandlerError::Defer(token)) => {
                        println!("vfsd-trace: mmap-reply file={file} defer={token}")
                    }
                }
            }
            result
        }

        fn mmap_call_inner(
            &mut self,
            file: u64,
            offset: u64,
            len: u64,
            shared: bool,
        ) -> HandlerResult<CallReply> {
            let (target, root, mount_id) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (
                    entry.file.target,
                    entry.file.root_listing,
                    entry.file.mount_id,
                )
            };
            if root {
                return Err(Errno::EISDIR.into());
            }
            if len == 0 || !offset.is_multiple_of(PAGE as u64) || !len.is_multiple_of(PAGE as u64) {
                return Err(Errno::EINVAL.into());
            }
            let stat = Client::new().fstat(target)?;
            if offset >= stat.size {
                return Err(Errno::EINVAL.into());
            }
            // NetBSD vfs_vnops.c:1063 selects `vp->v_uobj` for every file mmap, and DragonFly
            // vnode_pager.c:102 `vnode_pager_alloc` reuses `vp->v_object`; one file has one page
            // cache. Raven keys that object by mount+inode and the mapping window, and attaches
            // the existing object for a second mapping instead of creating a private cache.
            if let Some(object) = self.pager_for(mount_id, stat.ino, offset, len, shared) {
                let mut reply = CallReply::new(0);
                reply.attach(
                    object,
                    Rights::READ.union(Rights::WRITE).union(Rights::MAP).bits(),
                )?;
                if let Some(region) = self.pagers.get_mut(&object.0)
                    && !region.owners.contains(&file)
                {
                    region.owners.push(file);
                }
                return Ok(reply);
            }
            let object = raven_rt::sys::memobj_create_pager(
                usize::try_from(len).map_err(|_| Errno::ENOMEM)?,
            )?;
            // The pager keeps its own target so the page source survives the close of the open
            // description that created the mapping.
            let target = match raven_rt::sys::handle_dup(target, Rights::ALL) {
                Ok(target) => target,
                Err(error) => {
                    let _ = raven_rt::sys::handle_close(object);
                    return Err(error.into());
                }
            };
            self.pagers.insert(
                object.0,
                PagerRegion {
                    mount_id,
                    ino: stat.ino,
                    target,
                    offset,
                    len,
                    shared,
                    object,
                    owners: vec![file],
                },
            );
            let mut reply = CallReply::new(0);
            reply.attach(
                object,
                Rights::READ.union(Rights::WRITE).union(Rights::MAP).bits(),
            )?;
            Ok(reply)
        }

        /// Return the existing pager object for one file window, if a mapping created it.
        fn pager_for(
            &self,
            mount_id: u32,
            ino: u64,
            offset: u64,
            len: u64,
            shared: bool,
        ) -> Option<Handle> {
            self.pagers
                .values()
                .find(|region| {
                    region.mount_id == mount_id
                        && region.ino == ino
                        && region.offset == offset
                        && region.len == len
                        && region.shared == shared
                })
                .map(|region| region.object)
        }

        fn pager_call(&mut self, packet: PagerPacket) -> HandlerResult<CallReply> {
            let Some((target, region_offset)) = self
                .pagers
                .get(&packet.object)
                .map(|region| (region.target, region.offset))
            else {
                return Err(Errno::EIO.into());
            };
            if packet.len != PAGE as u64 {
                return Err(Errno::EINVAL.into());
            }
            let page_offset = usize::try_from(packet.offset).map_err(|_| Errno::EINVAL)?;
            let file_offset = region_offset
                .checked_add(packet.offset)
                .ok_or(Errno::EINVAL)?;
            let stat = Client::new().fstat(target)?;
            match packet.kind {
                PAGE_IN => {
                    if file_offset >= stat.size {
                        return Err(Errno::EIO.into());
                    }
                    let want = usize::try_from((stat.size - file_offset).min(PAGE as u64))
                        .map_err(|_| Errno::EINVAL)?;
                    let mut page = [0u8; PAGE];
                    let read = Client::new().pread(target, file_offset, &mut page[..want])?;
                    if read < want {
                        return Err(Errno::EIO.into());
                    }
                    raven_rt::sys::pager_supply(Handle(packet.object), page_offset, &page[..want])?;
                    Ok(CallReply::new(0))
                }
                PAGE_OUT => {
                    let address = raven_rt::sys::map_object(
                        Handle(packet.object),
                        page_offset,
                        PAGE,
                        prot::READ,
                        true,
                    )?;
                    let mut page = [0u8; PAGE];
                    raven_rt::sys::copy_from_mapping(address, &mut page);
                    let _ = raven_rt::sys::unmap(Handle::SELF_PROC, address, PAGE);
                    if file_offset < stat.size {
                        let want = usize::try_from((stat.size - file_offset).min(PAGE as u64))
                            .map_err(|_| Errno::EINVAL)?;
                        let written = Client::new().pwrite(target, file_offset, &page[..want])?;
                        if written < want {
                            return Err(Errno::EIO.into());
                        }
                    }
                    Ok(CallReply::new(0))
                }
                _ => Err(Errno::EINVAL.into()),
            }
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
                // Check the target before its parent: a mount root has an empty suffix, and its
                // parent lives outside the mounted scheme. `open("/dev/null", O_CREAT)` must
                // treat the existing device as the target instead of testing `null:` as a
                // directory.
                match self.target_stat(scheme, suffix) {
                    Ok(stat) => return check_stat_access(creds, stat, flags),
                    Err(Errno::ENOENT) => {
                        self.check_parent(scheme, suffix, creds)?;
                        return Ok(());
                    }
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
            let mut buffer = [0u8; DIR_BUFFER];
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

        /// Resolve a path CALL against the caller's root captured at open time.
        ///
        /// Absolute payload paths must not be resolved against the global mount table: a process
        /// that chrooted keeps its root on every path CALL through the already-open description.
        fn call_path(&self, root: &str, base: &str, path: &str) -> Result<String, Errno> {
            if root == "/" {
                self.absolute(base, path)
            } else {
                self.normalize_under_root(root, base, path)
            }
        }

        fn forward_one(
            &self,
            call: Call<'_>,
            base: &str,
            root: &str,
            response: &mut [u8],
            creds: &Credentials,
        ) -> Result<usize, Errno> {
            let raw = single_path(&call).ok_or(Errno::EINVAL)?;
            let path = self.call_path(root, base, &bytes_to_string(raw)?)?;
            let (mount_id, suffix) = self.table.resolve(&path)?;
            let scheme = self
                .table
                .get(mount_id)
                .ok_or(Errno::ENOENT)?
                .scheme
                .clone();
            match call {
                Call::Mkdir { .. }
                | Call::Symlink { .. }
                | Call::Mkfifo { .. }
                | Call::Mknod { .. } => {
                    self.check_parent(&scheme, suffix, creds)?;
                }
                Call::Unlink { .. } | Call::Rmdir { .. } => {
                    self.check_parent(&scheme, suffix, creds)?;
                    let target = self.target_lstat(&scheme, suffix)?;
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
                Call::Chown { .. } | Call::Lchown { .. } | Call::Utimens { .. } => {
                    self.check_traverse(&scheme, parent_suffix(suffix), creds)?;
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

        fn forward_link(
            &self,
            path: &str,
            target: &str,
            response: &mut [u8],
            creds: &Credentials,
        ) -> Result<usize, Errno> {
            if !self.table.same_mount(path, target)? {
                return Err(Errno::EXDEV);
            }
            let (mount_id, suffix) = self.table.resolve(path)?;
            let (_, target_suffix) = self.table.resolve(target)?;
            if suffix.is_empty() || target_suffix.is_empty() {
                return Err(Errno::EINVAL);
            }
            let scheme = self
                .table
                .get(mount_id)
                .ok_or(Errno::ENOENT)?
                .scheme
                .clone();
            self.check_parent(&scheme, suffix, creds)?;
            let client = Client::new();
            let root = client.open(&scheme, READ)?;
            let request = CallRequest::new(Call::Link {
                path: suffix.as_bytes(),
                target: target_suffix.as_bytes(),
            });
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
            let source = self.target_lstat(&scheme, suffix)?;
            let source_parent = self.target_stat(&scheme, parent_suffix(suffix))?;
            if !creds.sticky_delete_allowed((source_parent.mode & 0o7777) as u16, source.uid) {
                return Err(Errno::EPERM);
            }
            if let Ok(dest) = self.target_lstat(&scheme, to_suffix) {
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
            | Call::Chown { path, .. }
            | Call::Lchown { path, .. }
            | Call::Mkfifo { path, .. }
            | Call::Mknod { path, .. }
            | Call::Utimens { path, .. }
            | Call::Lstat { path }
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
        let mut scratch = vec![0u8; PROXY_CHUNK];
        let result = crate::chunk::copy_chunks(buffer, &mut scratch, |done, chunk| match offset {
            Some(base) => {
                let position = base.checked_add(done as u64).ok_or(Errno::EOVERFLOW)?;
                client.pread(target, position, chunk)
            }
            None => client.read(target, 0, chunk),
        });
        match result {
            Ok(count) => Ok(count),
            Err(crate::chunk::ChunkError::Io(error)) => Err(error),
            Err(crate::chunk::ChunkError::Overrun) => Err(Errno::EPROTO),
        }
    }

    /// Forward a write through a local buffer; see [`copy_read`].
    fn copy_write(
        client: &Client,
        target: Handle,
        offset: Option<u64>,
        buffer: &[u8],
    ) -> Result<usize, Errno> {
        let mut scratch = vec![0u8; PROXY_CHUNK];
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
            Call::Chown { uid, gid, .. } => Call::Chown {
                uid: *uid,
                gid: *gid,
                path,
            },
            Call::Lchown { uid, gid, .. } => Call::Lchown {
                uid: *uid,
                gid: *gid,
                path,
            },
            Call::Mkfifo { mode, .. } => Call::Mkfifo { mode: *mode, path },
            Call::Mknod { mode, dev, .. } => Call::Mknod {
                mode: *mode,
                dev: *dev,
                path,
            },
            Call::Utimens {
                atime,
                mtime,
                follow,
                ..
            } => Call::Utimens {
                path,
                atime: *atime,
                mtime: *mtime,
                follow: *follow,
            },
            Call::Lstat { .. } => Call::Lstat { path },
            Call::Symlink { target, .. } => Call::Symlink { path, target },
            _ => return Err(Errno::EINVAL),
        })
    }

    fn errno_from_status(status: i32) -> Errno {
        if (-4095..0).contains(&status) {
            Errno(u16::try_from(-status).unwrap_or(Errno::EIO.0))
        } else {
            Errno::EIO
        }
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
            if path == "__diag__" || path == "/__diag__" {
                self.dump_diag();
                return Err(Errno::EINVAL.into());
            }
            self.probe.record(ctx.trace_id, OPEN_OPCODE);
            let (path, requires_dir) = self.normalize_request_path(path)?;
            let directory = requires_dir || flags.directory();
            let target_flags = OpenFlags::from_wire(flags.to_wire() & !WIRE_OPEN_DIRECTORY)
                .map_err(|_| Errno::EINVAL)?;
            let mut path = path;
            let mut root = String::from("/");
            if let Ok(root_len) = usize::try_from(ctx.root_len)
                && root_len > 0
            {
                let open_root = path.get(..root_len).ok_or(Errno::EINVAL)?.to_string();
                if open_root != "/" {
                    path = self.resolve_rooted(&path, &open_root, !target_flags.nofollow())?;
                }
                root = open_root;
            }
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
            let target_handle = target.0;
            let path_for_trace = if self.trace {
                path.clone()
            } else {
                String::new()
            };
            let id = self.opens.insert(
                Proxy {
                    target,
                    mount_id,
                    root_listing: path == "/",
                    path,
                    root,
                    merged: None,
                    mount_generation: self.mount_generation,
                },
                flags,
            )?;
            if self.trace {
                println!(
                    "vfsd-trace: open id={id} path={} target={} trace={} req={}",
                    path_for_trace, target_handle, ctx.trace_id, ctx.request.0
                );
            }
            self.note_diag(1, u64::from(target_handle), 0, 0, ctx.request.0, true);
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

        fn read_with(
            &mut self,
            file: u64,
            offset: u64,
            buffer: &mut [u8],
            ctx: &Ctx,
        ) -> HandlerResult<usize> {
            self.note_diag(
                2,
                file,
                offset,
                u64::try_from(buffer.len()).unwrap_or(0),
                ctx.request.0,
                true,
            );
            self.probe.record(ctx.trace_id, READ_OPCODE);
            if !self.trace {
                return self.read(file, offset, buffer);
            }
            let target = self.opens.get(file).map_or(0, |entry| entry.file.target.0);
            println!(
                "vfsd-trace: read file={file} target={target} off={offset} len={} ptr={:p} head={:?} trace={} req={}",
                buffer.len(),
                buffer.as_ptr(),
                head(buffer),
                ctx.trace_id,
                ctx.request.0
            );
            let result = self.read(file, offset, buffer);
            match &result {
                Ok(bytes) => println!(
                    "vfsd-trace: read-reply file={file} target={target} bytes={bytes} head={:?}",
                    head(buffer)
                ),
                Err(HandlerError::Errno(error)) => println!(
                    "vfsd-trace: read-reply file={file} target={target} errno={}",
                    error.description()
                ),
                Err(HandlerError::Defer(token)) => {
                    println!("vfsd-trace: read-reply file={file} target={target} defer={token}")
                }
            }
            result
        }

        fn write(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            if offset != 0 {
                return Err(Errno::EINVAL.into());
            }
            let target = self.opens.get(file).ok_or(Errno::EBADF)?.file.target;
            Ok(copy_write(&Client::new(), target, None, buffer)?)
        }

        fn write_with(
            &mut self,
            file: u64,
            offset: u64,
            buffer: &[u8],
            ctx: &Ctx,
        ) -> HandlerResult<usize> {
            self.note_diag(
                3,
                file,
                offset,
                u64::try_from(buffer.len()).unwrap_or(0),
                ctx.request.0,
                true,
            );
            if !self.trace {
                return self.write(file, offset, buffer);
            }
            let target = self.opens.get(file).map_or(0, |entry| entry.file.target.0);
            println!(
                "vfsd-trace: write file={file} target={target} off={offset} len={} ptr={:p} head={:?} trace={} req={}",
                buffer.len(),
                buffer.as_ptr(),
                head(buffer),
                ctx.trace_id,
                ctx.request.0
            );
            let result = self.write(file, offset, buffer);
            match &result {
                Ok(bytes) => println!(
                    "vfsd-trace: write-reply file={file} target={target} bytes={bytes} head={:?}",
                    head(buffer)
                ),
                Err(HandlerError::Errno(error)) => println!(
                    "vfsd-trace: write-reply file={file} target={target} errno={}",
                    error.description()
                ),
                Err(HandlerError::Defer(token)) => {
                    println!("vfsd-trace: write-reply file={file} target={target} defer={token}")
                }
            }
            result
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

        fn pread_with(
            &mut self,
            file: u64,
            offset: u64,
            buffer: &mut [u8],
            ctx: &Ctx,
        ) -> HandlerResult<usize> {
            self.probe.record(ctx.trace_id, READ_OPCODE);
            self.pread(file, offset, buffer)
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
                let target = proxy.target;
                let _ = Client::new().close(target);
                // A pager object is shared by every open description that mapped the same file
                // window. Drop only this owner; release the object and its private target when
                // the last owner closes. The target is the last duplicate of this open, but the
                // pager holds its own duplicate, so another owner's page source stays valid.
                let mut release = Vec::new();
                for (object, region) in self.pagers.iter_mut() {
                    if let Some(index) = region.owners.iter().position(|owner| *owner == file) {
                        region.owners.swap_remove(index);
                    }
                    if region.owners.is_empty() {
                        release.push(*object);
                    }
                }
                for object in release {
                    if let Some(region) = self.pagers.remove(&object) {
                        let _ = raven_rt::sys::handle_close(region.object);
                        let _ = raven_rt::sys::handle_close(region.target);
                    }
                }
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
            if let Ok(packet) = PagerPacket::decode(request) {
                return self.pager_call(packet);
            }
            let decoded = CallRequest::decode(request).map_err(|_| Errno::EPROTO)?;
            if let Call::Mmap {
                offset,
                len,
                shared,
            } = decoded.call
            {
                return self.mmap_call(file, offset, len, shared);
            }
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
            let (target, base, root) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (
                    entry.file.target,
                    entry.file.path.clone(),
                    entry.file.root.clone(),
                )
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
                    let before = self.mount_generation;
                    if path == "/" {
                        // K176: a mount at `/` is the live root pivot. Open files keep their
                        // targets; the replacement table keeps /dev, /tmp and every other
                        // non-root mount.
                        let replacement = crate::pivot::pivot_root(&self.table, &scheme)?;
                        self.table = replacement;
                    } else {
                        self.table.mount(&path, &scheme)?;
                    }
                    self.bump_mounts();
                    if self.trace {
                        println!(
                            "vfsd-trace: mount path={path} scheme={scheme} gen={before} result=true"
                        );
                        println!("vfsd-trace: mount-bump gen={}", self.mount_generation);
                    }
                    status(response, Ok(()))
                }
                Call::Umount { path } => {
                    if !creds.is_root() {
                        return Err(Errno::EPERM.into());
                    }
                    let path = bytes_to_string(path)?;
                    let path = normalized(&path)?;
                    let before = self.mount_generation;
                    let result = self.table.umount(&path, 0);
                    if self.trace {
                        println!(
                            "vfsd-trace: umount path={path} gen={before} result={}",
                            result.is_ok()
                        );
                    }
                    result?;
                    self.bump_mounts();
                    if self.trace {
                        println!("vfsd-trace: umount-bump gen={}", self.mount_generation);
                    }
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
                Call::Statfs => match proxy_call(&Client::new(), target, &decoded, response) {
                    Ok(written) => Ok(written),
                    Err(Errno::ENOSYS) => {
                        // Device schemes such as null:, zero: and rand: do not account for
                        // blocks or inodes. Report zero capacity instead of resolving another
                        // mount (previously `/`/initfs) and inheriting its 41 MiB numbers.
                        let statfs = StatFs::default();
                        Ok(CallResponse::StatFs { status: 0, statfs }
                            .encode(response)
                            .map_err(|_| HandlerError::Errno(Errno::EPROTO))?)
                    }
                    Err(error) => Err(error.into()),
                },
                Call::Truncate { .. } | Call::Fchown { .. } => {
                    let stat = Client::new().fstat(target)?;
                    if matches!(decoded.call, Call::Truncate { .. })
                        && !creds.permitted(
                            (stat.mode & 0o7777) as u16,
                            stat.uid,
                            stat.gid,
                            ACCESS_WRITE,
                        )
                    {
                        return Err(Errno::EACCES.into());
                    }
                    Ok(proxy_call(&Client::new(), target, &decoded, response)?)
                }
                Call::Statvfs => Ok(proxy_call(&Client::new(), target, &decoded, response)?),
                Call::Link { path, target } => {
                    let path = self.call_path(&root, &base, &bytes_to_string(path)?)?;
                    let target = self.call_path(&root, &base, &bytes_to_string(target)?)?;
                    Ok(self.forward_link(&path, &target, response, creds)?)
                }
                Call::Rename { from, to } => {
                    let from = self.call_path(&root, &base, &bytes_to_string(from)?)?;
                    let to = self.call_path(&root, &base, &bytes_to_string(to)?)?;
                    Ok(self.forward_rename(&from, &to, response, creds)?)
                }
                Call::Access { mode, path } => {
                    let path = self.call_path(&root, &base, &bytes_to_string(path)?)?;
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
                | Call::Chmod { .. }
                | Call::Chown { .. }
                | Call::Lchown { .. }
                | Call::Mkfifo { .. }
                | Call::Mknod { .. }
                | Call::Utimens { .. }
                | Call::Lstat { .. } => {
                    Ok(self.forward_one(decoded.call, &base, &root, response, creds)?)
                }
                Call::Mmap { .. } => status(response, Err(Errno::EINVAL)),
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

    fn main(args: raven_rt::Args) -> i32 {
        let trace = args.env_get("VFSD_TRACE").is_some();
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
        let mut handler = VfsHandler::new(table, trace);
        server.run(&mut handler)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
