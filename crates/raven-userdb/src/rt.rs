// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The target filesystem and console adapter.
//!
//! This module is compiled only for `target_os = "none"`. It maps the `raven-rt` file API onto
//! [`DbIo`], reads prompts from `tty:console`, and creates a new account's home from `/etc/skel`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_proto_fs::{
    DT_REG, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY, S_IFDIR, S_IFMT,
};
use raven_rt::fs;

use crate::error::DbError;
use crate::store::DbIo;

/// The real `raven-rt` filesystem.
#[derive(Clone, Copy, Debug, Default)]
pub struct RtIo;

impl DbIo for RtIo {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, DbError> {
        fs::read_to_vec(path).map_err(DbError::Io)
    }

    fn create_exclusive(&mut self, path: &str) -> Result<(), DbError> {
        match fs::open(path, O_WRONLY | O_CREAT | O_EXCL) {
            Ok(file) => file.close().map_err(DbError::Io),
            Err(Errno::EEXIST) => Err(DbError::Busy),
            Err(error) => Err(DbError::Io(error)),
        }
    }

    fn remove(&mut self, path: &str) -> Result<(), DbError> {
        match fs::unlink(path) {
            Ok(()) | Err(Errno::ENOENT) => Ok(()),
            Err(error) => Err(DbError::Io(error)),
        }
    }

    fn write_new(&mut self, path: &str, data: &[u8], mode: u32) -> Result<(), DbError> {
        write_file(path, data, mode)
    }

    fn chmod(&mut self, path: &str, mode: u32) -> Result<(), DbError> {
        fs::chmod(path, mode).map_err(DbError::Io)
    }

    fn sync(&mut self) -> Result<(), DbError> {
        fs::sync().map_err(DbError::Io)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), DbError> {
        fs::rename(from, to).map_err(DbError::Io)
    }

    fn pause(&mut self) {
        let _ = raven_rt::sys::nanosleep(10_000_000);
    }
}

/// Read one line from `tty:console` after writing `prompt`.
///
/// The console discipline is canonical, so one read returns one complete line. The current console
/// server echoes the typed characters; disabling echo needs the terminal-mode work in U3. The
/// password is never printed by this function.
///
/// # Errors
/// An I/O error, invalid UTF-8, or an empty read.
pub fn read_line(prompt: &str) -> Result<String, DbError> {
    const MAX_LINE: usize = 1024;
    let mut file = fs::open("tty:console", O_RDWR).map_err(DbError::Io)?;
    file.write(prompt.as_bytes()).map_err(DbError::Io)?;
    let mut buffer = [0u8; 256];
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let read = file.read(&mut buffer).map_err(DbError::Io)?;
        if read == 0 {
            break;
        }
        let chunk = buffer.get(..read).unwrap_or(&[]);
        if !oversized {
            if line.len().saturating_add(read) > MAX_LINE {
                oversized = true;
            } else {
                line.extend_from_slice(chunk);
            }
        }
        if chunk.contains(&b'\n') {
            break;
        }
    }
    let _ = file.close();
    if oversized {
        return Err(DbError::Io(Errno::E2BIG));
    }
    if line.is_empty() {
        return Err(DbError::Io(Errno::EIO));
    }
    let text = core::str::from_utf8(&line).map_err(|_| DbError::Io(Errno::EILSEQ))?;
    Ok(text.trim_end_matches(['\r', '\n']).to_string())
}

/// Write a whole file with an explicit mode and flush it.
///
/// # Errors
/// An I/O error while creating, writing, changing the mode or flushing.
pub fn write_file(path: &str, data: &[u8], mode: u32) -> Result<(), DbError> {
    let mut file = fs::open(path, O_WRONLY | O_CREAT | O_TRUNC).map_err(DbError::Io)?;
    // Apply the final mode before writing so a shadow temporary never holds secret bytes with
    // the server's default 0644 creation mode.
    fs::chmod(path, mode).map_err(DbError::Io)?;
    let mut offset = 0usize;
    while offset < data.len() {
        let written = file
            .write(data.get(offset..).unwrap_or(&[]))
            .map_err(DbError::Io)?;
        if written == 0 {
            return Err(DbError::Io(Errno::EIO));
        }
        offset = offset.saturating_add(written);
    }
    file.close().map_err(DbError::Io)?;
    fs::sync().map_err(DbError::Io)
}

/// Create a mode-0700 home directory and copy regular files from `/etc/skel`.
///
/// The VFS has no `chown` in this slice, so the files stay owned by the caller (root). U3 owns the
/// uid/gid ownership of the home; the mode is already the 0.2.0 contract.
///
/// # Errors
/// An I/O error while creating the directory or copying a skeleton file.
pub fn create_home(path: &str) -> Result<(), DbError> {
    match fs::stat(path) {
        Ok(stat) if stat.mode & S_IFMT == S_IFDIR => return Ok(()),
        Ok(_) => return Err(DbError::Io(Errno::EEXIST)),
        Err(Errno::ENOENT) => {}
        Err(error) => return Err(DbError::Io(error)),
    }
    if let Err(error) = fs::mkdir(path, 0o700) {
        if error != Errno::ENOENT {
            return Err(DbError::Io(error));
        }
        if let Some((parent, _)) = path.rsplit_once('/')
            && !parent.is_empty()
        {
            fs::mkdir(parent, 0o755).map_err(DbError::Io)?;
        }
        fs::mkdir(path, 0o700).map_err(DbError::Io)?;
    }
    copy_skel(path)
}

fn copy_skel(home: &str) -> Result<(), DbError> {
    let entries = match fs::read_dir("/etc/skel") {
        Ok(entries) => entries,
        Err(Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(DbError::Io(error)),
    };
    for entry in entries {
        if entry.kind != DT_REG {
            continue;
        }
        let source = format!("/etc/skel/{}", entry.name);
        let data = fs::read_to_vec(&source).map_err(DbError::Io)?;
        let target = format!("{home}/{}", entry.name);
        write_file(&target, &data, 0o644)?;
    }
    Ok(())
}

/// Read `N` bytes from a device scheme such as `rand:`.
///
/// The `rand:` server is the 0.2.0 entropy source. It is explicitly not cryptographically secure
/// yet; a password salt only needs to be unique, not secret, so this is the intended use.
///
/// # Errors
/// An I/O error, a short read, or an invalid scheme path.
pub fn random_bytes<const N: usize>(path: &str) -> Result<[u8; N], DbError> {
    let mut file = fs::open(path, O_RDONLY).map_err(DbError::Io)?;
    let mut bytes = [0u8; N];
    let mut offset = 0usize;
    while offset < N {
        let read = file
            .read(bytes.get_mut(offset..).unwrap_or(&mut []))
            .map_err(DbError::Io)?;
        if read == 0 {
            return Err(DbError::Io(Errno::EIO));
        }
        offset = offset.saturating_add(read);
    }
    let _ = file.close();
    Ok(bytes)
}

/// Format a `KEY=VALUE` environment string.
#[must_use]
pub fn env_line(key: &str, value: &str) -> String {
    format!("{key}={value}")
}
