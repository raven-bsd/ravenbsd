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
use raven_abi::types::{Handle, Rights};
use raven_proto_fs::{
    Call, CallRequest, CallResponse, DirEntry as ProtoDirEntry, DirEntryIter, MAX_PATH,
    O_DIRECTORY, O_RDONLY, O_WRONLY, OP_READLINK, OpenFlags, Stat, StatFs, normalize_absolute,
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

/// An open `file:` handle.
///
/// The filesystem contract is sequential: the server owns the open-file
/// description cursor and advances it on each `READ`/`WRITE`.  The client does
/// not keep a second offset; a duplicate made with [`File::try_clone`] shares the
/// server-side cursor because `DUP` shares the open-file description.
pub struct File {
    handle: Option<Handle>,
}

impl File {
    /// Duplicate the handle.  The new handle shares the open-file description
    /// and therefore the cursor; each handle closes independently.
    pub fn try_clone(&self) -> Result<Self, Errno> {
        let handle = self.handle.ok_or(Errno::EBADF)?;
        let duplicate = raven_scheme::Client::new().dup(handle, Rights::ALL.bits())?;
        Ok(Self {
            handle: Some(duplicate),
        })
    }

    /// Read up to `buffer.len()` bytes from the current cursor.
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Errno> {
        let handle = self.handle.ok_or(Errno::EBADF)?;
        raven_scheme::Client::new().read(handle, 0, buffer)
    }

    /// Write `buffer` at the current cursor.
    pub fn write(&mut self, buffer: &[u8]) -> Result<usize, Errno> {
        let handle = self.handle.ok_or(Errno::EBADF)?;
        raven_scheme::Client::new().write(handle, 0, buffer)
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

/// Return the loader-provided `AT_RAVEN_CWD`, else `PWD`, else `/`.
///
/// The spawner publishes the parent's working directory as `AT_RAVEN_CWD`; a child that reads
/// only `PWD` inherits the wrong directory when the shell exports a stale value.
pub fn cwd() -> String {
    let args = crate::args::current();
    if let Some(path) = args.aux_string(crate::spawn_attrs::AT_RAVEN_CWD)
        && path.starts_with('/')
    {
        return path.to_string();
    }
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

/// Resolve `path` and qualify a plain path with the `file:` scheme.
fn scheme_path(path: &str) -> String {
    let resolved = resolve(path);
    if has_scheme(&resolved) {
        resolved
    } else {
        alloc_crate::format!("file:{resolved}")
    }
}

/// Normalise an absolute path.
pub fn normalize(path: &str) -> Result<String, Errno> {
    let mut buffer = [0u8; MAX_PATH];
    normalize_absolute(path, &mut buffer)
        .map(|text| text.to_string())
        .map_err(|error| error.errno())
}

/// Open a path with Linux-valued open flags.
///
/// [`OpenFlags::from_linux`] validates the flags and translates them to the K66
/// wire encoding.  The kernel must carry that encoding before S37 lands; until
/// then a flag outside the legacy READ/WRITE/CREATE bits is rejected with
/// `EINVAL` by the kernel.
pub fn open(path: &str, flags: u32) -> Result<File, Errno> {
    let resolved = scheme_path(path);
    let wire = OpenFlags::from_linux(flags).map_err(|error| error.errno())?;
    let handle = raven_scheme::Client::new().open(&resolved, u32::from(wire.to_wire()))?;
    Ok(File {
        handle: Some(handle),
    })
}

/// Translate Linux open flags to the raw wire bits.
pub fn s11_flags(flags: u32) -> Result<u32, Errno> {
    OpenFlags::from_linux(flags)
        .map(|wire| u32::from(wire.to_wire()))
        .map_err(|error| error.errno())
}

/// Read the entries of a directory path.
///
/// Directory reads are sequential per open-file description.  The server
/// invalidates the description's cursor with `EINVAL` if the directory changes
/// between pages; reopen the path to restart from the beginning.
pub fn read_dir(path: &str) -> Result<Vec<DirEntry>, Errno> {
    let mut file = open(path, O_RDONLY | O_DIRECTORY)?;
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
        CallResponse::Readlink { status, .. }
        | CallResponse::Status(status)
        | CallResponse::StatFs { status, .. }
        | CallResponse::StatVfs { status, .. }
        | CallResponse::Stat { status, .. } => Err(errno_from_status(status)),
    }
}

/// Change permission bits.
pub fn chmod(path: &str, mode: u32) -> Result<(), Errno> {
    root_call(Call::Chmod {
        mode,
        path: path.as_bytes(),
    })
}

/// Check `mode` (`4` read, `2` write, `1` execute) at `path` for the caller.
pub fn access(path: &str, mode: u32) -> Result<(), Errno> {
    root_call(Call::Access {
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
        CallResponse::StatFs { status, .. }
        | CallResponse::StatVfs { status, .. }
        | CallResponse::Stat { status, .. }
        | CallResponse::Status(status) => Err(errno_from_status(status)),
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
        CallResponse::Readlink { status, .. }
        | CallResponse::StatFs { status, .. }
        | CallResponse::StatVfs { status, .. }
        | CallResponse::Stat { status, .. } => status_result(status),
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
#[path = "fs_ut.rs"]
mod tests;
