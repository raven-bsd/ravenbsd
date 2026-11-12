// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `vfsd`: the `file:` namespace server.
//!
//! It holds the mount table and forwards `OPEN` by attaching the owning
//! filesystem server's handle (S25).  Path operations are proxied through
//! `CALL`; reads and writes go directly to the attached server after `OPEN`.
//! The `/` root handle is the one exception: its `READ` merges the root
//! server's entries with the names of mounts directly below `/`.

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
        Call, CallRequest, CallResponse, DIRENT_ALIGN, DIRENT_HEADER_LEN, DT_DIR, DirEntry,
        DirEntryHeader, DirEntryIter, MAX_PATH, OpenFlags, S_IFDIR, S_IFMT, Stat,
        WIRE_OPEN_DIRECTORY, normalize_request,
    };
    use raven_rt::entry;
    use raven_rt::println;
    use raven_scheme::{
        Client, Ctx, Handler, HandlerResult, Kernel, OpenTable, Opened, RtKernel, Server,
    };
    use raven_vfs_core::{DEFAULT_MOUNTS, MountTable, normalized, parse_mounts};

    entry!(main);

    const READ: u32 = 1;
    const RETRIES: usize = 500;
    const RETRY_NS: u64 = 10_000_000;
    const BUFFER: usize = 4096;

    /// The real kernel, plus vfsd's ownership rule for Open attachments.
    ///
    /// `Opened::Attach` hands the target handle to the client.  The reply send
    /// installs a client-side duplicate of the same SchemeFile; vfsd's source
    /// handle must then be closed.  The kernel SchemeFile keeps the server
    /// reference alive through the client duplicate, so the target server does
    /// not see a close until the client's last handle goes away.
    #[derive(Clone, Copy, Debug, Default)]
    struct VfsKernel {
        inner: RtKernel,
    }

    impl Kernel for VfsKernel {
        fn scheme_register(&self, name: &str) -> Result<(), Errno> {
            self.inner.scheme_register(name)
        }

        fn recv(&self, packet: &mut [u8]) -> Result<Handle, Errno> {
            self.inner.recv(packet)
        }

        fn recv_timeout(
            &self,
            packet: &mut [u8],
            timeout_ms: u64,
        ) -> Result<Option<Handle>, Errno> {
            self.inner.recv_timeout(packet, timeout_ms)
        }

        fn send(&self, request: Handle, packet: &[u8]) -> Result<(), Errno> {
            self.inner.send(request, packet)
        }

        fn send_attach(
            &self,
            request: Handle,
            packet: &[u8],
            attachments: &[raven_scheme::Attachment],
        ) -> Result<(), Errno> {
            let result = self.inner.send_attach(request, packet, attachments);
            for attachment in attachments {
                let _ = self.inner.close(Handle(attachment.handle));
            }
            result
        }
    }

    struct Proxy {
        target: Handle,
        path: String,
        root_listing: bool,
        merged: Option<Vec<u8>>,
    }

    struct VfsHandler {
        table: MountTable,
        opens: OpenTable<Proxy>,
    }

    impl VfsHandler {
        fn new(table: MountTable) -> Self {
            Self {
                table,
                opens: OpenTable::new(),
            }
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

        fn read_root(&mut self, file: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
            let (target, needs_build) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (entry.file.target, entry.file.merged.is_none())
            };
            if needs_build {
                let packed = self.build_root_listing(target)?;
                self.opens.get_mut(file).ok_or(Errno::EBADF)?.file.merged = Some(packed);
            }
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
            _ctx: &Ctx,
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
            if path == "/" {
                let id = self.opens.insert(
                    Proxy {
                        target,
                        path,
                        root_listing: true,
                        merged: None,
                    },
                    flags,
                )?;
                Ok(Opened::File(id))
            } else {
                Ok(Opened::Attach(target))
            }
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
                Ok(Client::new().read(target, 0, buffer)?)
            }
        }

        fn write(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            if offset != 0 {
                return Err(Errno::EINVAL.into());
            }
            let target = self.opens.get(file).ok_or(Errno::EBADF)?.file.target;
            Ok(Client::new().write(target, 0, buffer)?)
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
            let (target, base) = {
                let entry = self.opens.get(file).ok_or(Errno::EBADF)?;
                (entry.file.target, entry.file.path.clone())
            };
            let decoded = CallRequest::decode(request).map_err(|_| Errno::EPROTO)?;
            match decoded.call {
                Call::Mount { path, scheme } => {
                    let path = bytes_to_string(path)?;
                    let path = normalized(&path)?;
                    let scheme = bytes_to_string(scheme)?;
                    self.table.mount(&path, &scheme)?;
                    status(response, Ok(()))
                }
                Call::Umount { path } => {
                    let path = bytes_to_string(path)?;
                    let path = normalized(&path)?;
                    self.table.umount(&path, 0)?;
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
                    Ok(proxy_call(&Client::new(), target, &decoded, response)?)
                }
                Call::Rename { from, to } => {
                    let from = self.absolute(&base, &bytes_to_string(from)?)?;
                    let to = self.absolute(&base, &bytes_to_string(to)?)?;
                    Ok(self.forward_rename(&from, &to, response)?)
                }
                Call::Mkdir { .. }
                | Call::Unlink { .. }
                | Call::Rmdir { .. }
                | Call::Symlink { .. }
                | Call::Readlink { .. }
                | Call::Chmod { .. } => Ok(self.forward_one(decoded.call, &base, response)?),
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
        let mut server = match Server::register_with(VfsKernel::default(), "file") {
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
