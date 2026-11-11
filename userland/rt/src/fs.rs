// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `PWD`-relative file access through the `file:` scheme.
//!
//! Paths without a scheme are resolved against `PWD` and opened on `file:`. A path containing a
//! `:` before its first `/` is a scheme path (`tty:console`, `file:/tmp/a`) and is passed through
//! unchanged. The client side uses the K60 control records for directory and path operations.

use alloc_crate::string::{String, ToString};
use alloc_crate::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_fs::{
    Call, CallRequest, CallResponse, DirEntry as ProtoDirEntry, DirEntryIter, MAX_PATH, O_ACCMODE,
    O_CREAT, O_RDONLY, O_RDWR, O_WRONLY, OP_READLINK, Stat, StatFs, normalize_absolute,
};

/// One owned directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// Inode number.
    pub ino: u64,
    /// `DT_*` type.
    pub kind: u8,
    /// Entry name.
    pub name: String,
}

/// An open `file:` handle with a client-side offset.
pub struct File {
    handle: Option<Handle>,
    offset: u64,
}

impl File {
    /// Current client-side offset.
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Set the client offset for the next positioned read/write.
    pub fn seek(&mut self, offset: u64) {
        self.offset = offset;
    }

    /// Read up to `buffer.len()` bytes and advance the offset.
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Errno> {
        let handle = self.handle.ok_or(Errno::EBADF)?;
        let read = raven_scheme::Client::new().read(handle, self.offset, buffer)?;
        self.offset = self.offset.saturating_add(read as u64);
        Ok(read)
    }

    /// Write `buffer` and advance the offset.
    pub fn write(&mut self, buffer: &[u8]) -> Result<usize, Errno> {
        let handle = self.handle.ok_or(Errno::EBADF)?;
        let written = raven_scheme::Client::new().write(handle, self.offset, buffer)?;
        self.offset = self.offset.saturating_add(written as u64);
        Ok(written)
    }

    /// Read the `FSTAT` record.
    pub fn fstat(&self) -> Result<Stat, Errno> {
        let handle = self.handle.ok_or(Errno::EBADF)?;
        raven_scheme::Client::new().fstat(handle)
    }

    /// Read the rest of the file into a vector.
    pub fn read_to_vec(&mut self) -> Result<Vec<u8>, Errno> {
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = self.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..read]);
        }
        Ok(data)
    }

    /// Close the handle explicitly.
    pub fn close(mut self) -> Result<(), Errno> {
        let handle = self.handle.take().ok_or(Errno::EBADF)?;
        raven_scheme::Client::new().close(handle)
    }

    fn raw_handle(&self) -> Result<Handle, Errno> {
        self.handle.ok_or(Errno::EBADF)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = raven_scheme::Client::new().close(handle);
        }
    }
}

/// Return `PWD` or `/` when it is absent or empty.
pub fn cwd() -> String {
    let args = crate::args::current();
    match args.env_get("PWD") {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => "/".to_string(),
    }
}

/// True when `path` starts with a scheme name before any `/`.
pub fn has_scheme(path: &str) -> bool {
    match path.find(':') {
        Some(index) => !path[..index].contains('/'),
        None => false,
    }
}

/// Resolve `path` against `pwd`, returning a normalised absolute path or a scheme path.
pub fn resolve_with_pwd(path: &str, pwd: &str) -> String {
    if has_scheme(path) {
        return path.to_string();
    }
    let joined = if path.starts_with('/') {
        path.to_string()
    } else if pwd == "/" {
        alloc_crate::format!("/{path}")
    } else {
        alloc_crate::format!("{pwd}/{path}")
    };
    normalize(&joined).unwrap_or(joined)
}

/// Resolve `path` against [`cwd`].
pub fn resolve(path: &str) -> String {
    resolve_with_pwd(path, &cwd())
}

/// Normalise an absolute path.
pub fn normalize(path: &str) -> Result<String, Errno> {
    let mut buffer = [0u8; MAX_PATH];
    normalize_absolute(path, &mut buffer)
        .map(|text| text.to_string())
        .map_err(|error| error.errno())
}

/// Open a path with K60/Linux open flags.
///
/// The S11 `OPEN` syscall currently carries only read/write/create bits. `O_EXCL`, `O_TRUNC`,
/// `O_APPEND`, `O_DIRECTORY` and `O_NOFOLLOW` are accepted by this client API but cannot be
/// transmitted; servers that need them should use a `CALL` operation.
pub fn open(path: &str, flags: u32) -> Result<File, Errno> {
    let resolved = resolve(path);
    let s11 = s11_flags(flags)?;
    let handle = raven_scheme::Client::new().open(&resolved, s11)?;
    Ok(File {
        handle: Some(handle),
        offset: 0,
    })
}

/// Translate K60/Linux open flags to the S11 flag bits.
pub fn s11_flags(flags: u32) -> Result<u32, Errno> {
    let mut result = 0u32;
    match flags & O_ACCMODE {
        O_RDONLY => result |= 1,
        O_WRONLY => result |= 2,
        O_RDWR => result |= 3,
        _ => return Err(Errno::EINVAL),
    }
    if flags & O_CREAT != 0 {
        result |= 4;
    }
    Ok(result)
}

/// Read the entries of a directory path.
pub fn read_dir(path: &str) -> Result<Vec<DirEntry>, Errno> {
    let mut file = open(path, O_RDONLY)?;
    let mut entries = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        entries.extend(decode_dir_entries(&buffer[..read])?);
    }
    Ok(entries)
}

/// Decode one packed K60 directory buffer into owned entries.
pub fn decode_dir_entries(buffer: &[u8]) -> Result<Vec<DirEntry>, Errno> {
    let mut entries = Vec::new();
    for entry in DirEntryIter::new(buffer) {
        let entry: ProtoDirEntry<'_> = entry.map_err(|_| Errno::EPROTO)?;
        let name = String::from_utf8(entry.name.to_vec()).map_err(|_| Errno::EINVAL)?;
        entries.push(DirEntry {
            ino: entry.header.ino,
            kind: entry.header.kind,
            name,
        });
    }
    Ok(entries)
}

/// Stat a path.
pub fn stat(path: &str) -> Result<Stat, Errno> {
    open(path, O_RDONLY)?.fstat()
}

/// Read a whole file into a vector.
pub fn read_to_vec(path: &str) -> Result<Vec<u8>, Errno> {
    open(path, O_RDONLY)?.read_to_vec()
}

/// Create a directory.
pub fn mkdir(path: &str, mode: u32) -> Result<(), Errno> {
    root_call(Call::Mkdir {
        mode,
        path: path.as_bytes(),
    })
}

/// Remove a file.
pub fn unlink(path: &str) -> Result<(), Errno> {
    root_call(Call::Unlink {
        path: path.as_bytes(),
    })
}

/// Remove an empty directory.
pub fn rmdir(path: &str) -> Result<(), Errno> {
    root_call(Call::Rmdir {
        path: path.as_bytes(),
    })
}

/// Rename `from` to `to`.
pub fn rename(from: &str, to: &str) -> Result<(), Errno> {
    root_call(Call::Rename {
        from: from.as_bytes(),
        to: to.as_bytes(),
    })
}

/// Truncate an open file.
pub fn truncate(path: &str, size: u64) -> Result<(), Errno> {
    let file = open(path, O_WRONLY)?;
    let request = CallRequest::new(Call::Truncate { size });
    let mut response = [0u8; 64];
    let length =
        raven_scheme::Client::new().call_typed(file.raw_handle()?, &request, &mut response)?;
    check_response(request.op(), &response[..length])
}

/// Create a symbolic link.
pub fn symlink(path: &str, target: &str) -> Result<(), Errno> {
    root_call(Call::Symlink {
        path: path.as_bytes(),
        target: target.as_bytes(),
    })
}

/// Read a symbolic link target.
pub fn readlink(path: &str) -> Result<String, Errno> {
    let root = open("file:/", O_RDONLY)?;
    let request = CallRequest::new(Call::Readlink {
        path: path.as_bytes(),
    });
    let mut response = [0u8; 256];
    let length =
        raven_scheme::Client::new().call_typed(root.raw_handle()?, &request, &mut response)?;
    match CallResponse::decode(OP_READLINK, &response[..length]).map_err(|_| Errno::EPROTO)? {
        CallResponse::Readlink { status: 0, target } => {
            String::from_utf8(target.to_vec()).map_err(|_| Errno::EINVAL)
        }
        CallResponse::Readlink { status, .. } | CallResponse::Status(status) => {
            Err(errno_from_status(status))
        }
        CallResponse::StatFs { status, .. } => Err(errno_from_status(status)),
    }
}

/// Change permission bits.
pub fn chmod(path: &str, mode: u32) -> Result<(), Errno> {
    root_call(Call::Chmod {
        mode,
        path: path.as_bytes(),
    })
}

/// Flush filesystem state.
pub fn sync() -> Result<(), Errno> {
    root_call(Call::Sync)
}

/// Mount `scheme` at an absolute path.
pub fn mount(path: &str, scheme: &str) -> Result<(), Errno> {
    root_call(Call::Mount {
        path: path.as_bytes(),
        scheme: scheme.as_bytes(),
    })
}

/// Unmount an absolute path.
pub fn umount(path: &str) -> Result<(), Errno> {
    root_call(Call::Umount {
        path: path.as_bytes(),
    })
}

/// Read filesystem statistics.
pub fn statfs(path: &str) -> Result<StatFs, Errno> {
    let _ = path;
    let root = open("file:/", O_RDONLY)?;
    let request = CallRequest::new(Call::Statfs);
    let mut response = [0u8; 64];
    let length =
        raven_scheme::Client::new().call_typed(root.raw_handle()?, &request, &mut response)?;
    match CallResponse::decode(request.op(), &response[..length]).map_err(|_| Errno::EPROTO)? {
        CallResponse::StatFs { status: 0, statfs } => Ok(statfs),
        CallResponse::StatFs { status, .. } | CallResponse::Status(status) => {
            Err(errno_from_status(status))
        }
        CallResponse::Readlink { status, .. } => Err(errno_from_status(status)),
    }
}

fn root_call(call: Call<'_>) -> Result<(), Errno> {
    let root = open("file:/", O_RDONLY)?;
    let request = CallRequest::new(call);
    let mut response = [0u8; 64];
    let length =
        raven_scheme::Client::new().call_typed(root.raw_handle()?, &request, &mut response)?;
    check_response(request.op(), &response[..length])
}

fn check_response(op: u16, response: &[u8]) -> Result<(), Errno> {
    match CallResponse::decode(op, response).map_err(|_| Errno::EPROTO)? {
        CallResponse::Status(status) => status_result(status),
        CallResponse::Readlink { status, .. } | CallResponse::StatFs { status, .. } => {
            status_result(status)
        }
    }
}

fn status_result(status: i32) -> Result<(), Errno> {
    if status == 0 {
        Ok(())
    } else {
        Err(errno_from_status(status))
    }
}

fn errno_from_status(status: i32) -> Errno {
    if (-4095..0).contains(&status) {
        Errno(u16::try_from(-status).unwrap_or(Errno::EIO.0))
    } else {
        Errno::EIO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raven_proto_fs::{DIRENT_HEADER_LEN, DT_DIR, DT_REG, S_IFDIR, S_IFREG};

    #[test]
    fn resolve_absolute_path() {
        assert_eq!(resolve_with_pwd("/tmp/a", "/home/me"), "/tmp/a");
        assert_eq!(resolve_with_pwd("/tmp/../a", "/home/me"), "/a");
    }

    #[test]
    fn resolve_relative_path() {
        assert_eq!(resolve_with_pwd("a", "/tmp"), "/tmp/a");
        assert_eq!(resolve_with_pwd("a/../b", "/tmp"), "/tmp/b");
        assert_eq!(resolve_with_pwd("a", "/"), "/a");
    }

    #[test]
    fn resolve_keeps_scheme_paths() {
        assert_eq!(resolve_with_pwd("file:/tmp", "/home"), "file:/tmp");
        assert_eq!(resolve_with_pwd("tty:console", "/home"), "tty:console");
        assert!(!has_scheme("/tmp/a:b"));
        assert!(has_scheme("echo:hello"));
    }

    #[test]
    fn resolve_rejects_escape_fallback_keeps_input() {
        assert_eq!(resolve_with_pwd("../../x", "/tmp"), "/tmp/../../x");
    }

    #[test]
    fn normalize_uses_the_k60_helper() {
        assert_eq!(normalize("/a/./b/../c").unwrap(), "/a/c");
        assert_eq!(normalize("relative"), Err(Errno::EINVAL));
    }

    #[test]
    fn s11_flags_map_access_and_create() {
        assert_eq!(s11_flags(O_RDONLY).unwrap(), 1);
        assert_eq!(s11_flags(O_WRONLY).unwrap(), 2);
        assert_eq!(s11_flags(O_RDWR).unwrap(), 3);
        assert_eq!(s11_flags(O_RDWR | O_CREAT).unwrap(), 7);
        assert_eq!(s11_flags(3), Err(Errno::EINVAL));
    }

    #[test]
    fn decode_two_directory_entries() {
        let mut buffer = [0u8; 48];
        let first = ProtoDirEntry {
            header: raven_proto_fs::DirEntryHeader {
                ino: 1,
                kind: DT_REG,
                name_len: 1,
                pad: [0; 6],
            },
            name: b"a",
        };
        let second = ProtoDirEntry {
            header: raven_proto_fs::DirEntryHeader {
                ino: 2,
                kind: DT_DIR,
                name_len: 1,
                pad: [0; 6],
            },
            name: b"b",
        };
        let first_len = first.encode(&mut buffer).unwrap();
        second.encode(&mut buffer[first_len..]).unwrap();
        let entries = decode_dir_entries(&buffer).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a");
        assert_eq!(entries[1].name, "b");
        assert_eq!(entries[1].kind, DT_DIR);
    }

    #[test]
    fn decode_bad_directory_entry_is_eproto() {
        assert!(decode_dir_entries(&[0u8; 8]).is_err());
    }

    #[test]
    fn dir_entry_kind_maps_from_mode() {
        assert_eq!(raven_proto_fs::dirent_kind(S_IFREG | 0o644), DT_REG);
        assert_eq!(raven_proto_fs::dirent_kind(S_IFDIR | 0o755), DT_DIR);
    }

    #[test]
    fn header_size_matches_k60() {
        assert_eq!(DIRENT_HEADER_LEN, 16);
    }
}
