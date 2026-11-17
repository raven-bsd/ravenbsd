// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target filesystem implementation of the spool seam.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY, S_IFDIR, S_IFMT};
use raven_rt::fs;
use raven_tool_core::Errno;
use raven_tool_core::path::with_file_scheme;

use crate::spool::{Spool, SpoolError};

/// The target spool backed by `raven_rt::fs`.
pub struct TargetSpool;

impl Spool for TargetSpool {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, SpoolError> {
        fs::read_to_vec(&target(path)).map_err(map_error)
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), SpoolError> {
        let mut file = fs::open(&target(path), O_WRONLY | O_CREAT | O_TRUNC).map_err(map_error)?;
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file
                .write(data.get(offset..).unwrap_or(&[]))
                .map_err(map_error)?;
            if written == 0 {
                let _ = file.close();
                return Err(SpoolError::Io);
            }
            offset = offset.saturating_add(written);
        }
        file.close().map_err(map_error)
    }

    fn remove(&mut self, path: &str) -> Result<(), SpoolError> {
        fs::unlink(&target(path)).map_err(map_error)
    }

    fn list(&mut self, path: &str) -> Result<Vec<String>, SpoolError> {
        let entries = fs::read_dir(&target(path)).map_err(map_error)?;
        Ok(entries.into_iter().map(|entry| entry.name).collect())
    }

    fn create_dir_all(&mut self, path: &str) -> Result<(), SpoolError> {
        if path.is_empty() || path == "/" {
            return Ok(());
        }
        let mut current = String::new();
        for part in path.split('/') {
            if part.is_empty() {
                continue;
            }
            if current.is_empty() {
                if path.starts_with('/') {
                    current.push('/');
                }
            } else if !current.ends_with('/') {
                current.push('/');
            }
            current.push_str(part);
            match fs::mkdir(&target(&current), 0o755) {
                Ok(()) | Err(Errno::EEXIST) => {}
                Err(error) => return Err(map_error(error)),
            }
        }
        Ok(())
    }

    fn is_dir(&mut self, path: &str) -> Result<bool, SpoolError> {
        let stat = fs::stat(&target(path)).map_err(map_error)?;
        Ok(stat.mode & S_IFMT == S_IFDIR)
    }

    fn exists(&mut self, path: &str) -> bool {
        fs::stat(&target(path)).is_ok()
    }

    fn mode(&mut self, path: &str) -> Result<u32, SpoolError> {
        let stat = fs::stat(&target(path)).map_err(map_error)?;
        Ok(stat.mode & 0o7777)
    }

    fn set_mode(&mut self, path: &str, mode: u32) -> Result<(), SpoolError> {
        fs::chmod(&target(path), mode & 0o7777).map_err(map_error)
    }
}

/// Read a UTF-8 text file through the target filesystem.
///
/// # Errors
///
/// Returns [`SpoolError`] when the file cannot be read or is not UTF-8.
pub fn read_text(path: &str) -> Result<String, SpoolError> {
    let data = fs::read_to_vec(&target(path)).map_err(map_error)?;
    String::from_utf8(data).map_err(|_| SpoolError::InvalidPath)
}

fn target(path: &str) -> String {
    with_file_scheme(&fs::resolve(path))
}

fn map_error(error: Errno) -> SpoolError {
    if error == Errno::ENOENT {
        SpoolError::NotFound
    } else {
        SpoolError::Io
    }
}

/// Format the target's `uucp` default user from the environment.
#[must_use]
pub fn default_user(value: Option<&str>) -> String {
    value.unwrap_or("unknown").to_string()
}
