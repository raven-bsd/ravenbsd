// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Allocation-free path helpers shared by the servers.
//!
//! `vfsd` normalises absolute client paths before routing them. Filesystem servers receive paths
//! relative to their mount root and walk them one component at a time, rejecting `.` and `..` so
//! no client can escape the server's root with a relative suffix.

use core::fmt;

use crate::{Errno, ProtoError};

/// Public POSIX `PATH_MAX` in bytes, including the terminating NUL.
///
/// The public limit is 4096, so a conforming pathname carries at most
/// [`MAX_PATH`] bytes without its NUL terminator.
pub const PATH_MAX: usize = 4096;
/// Maximum path bytes carried by the protocol, excluding a terminating NUL.
pub const MAX_PATH: usize = PATH_MAX - 1;
/// Maximum single component length in bytes.
///
/// This is the strictest component limit of the supported filesystems (UFS/FFS
/// `MAXNAMLEN`); a filesystem reports its real limit through `StatVfs::name_max`.
pub const MAX_COMPONENT: usize = 255;

/// Why a path was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    /// The path is empty.
    Empty,
    /// An absolute path was required but the input does not start with `/`.
    NotAbsolute,
    /// A `..` component would escape the root.
    EscapesRoot,
    /// The path or output buffer is longer than [`MAX_PATH`].
    TooLong,
    /// One component is longer than [`MAX_COMPONENT`].
    ComponentTooLong,
    /// The path contains a `.` or `..` component where a name was required.
    DotComponent,
    /// The path contains a NUL or is otherwise malformed.
    Invalid,
}

impl PathError {
    /// The Linux errno a server should return for this failure.
    pub const fn errno(self) -> Errno {
        match self {
            Self::TooLong | Self::ComponentTooLong => Errno::ENAMETOOLONG,
            Self::Invalid | Self::Empty | Self::DotComponent | Self::EscapesRoot => Errno::EINVAL,
            Self::NotAbsolute => Errno::EINVAL,
        }
    }
}

impl From<PathError> for ProtoError {
    fn from(error: PathError) -> Self {
        match error {
            PathError::TooLong | PathError::ComponentTooLong => Self::LengthOutOfRange,
            PathError::Invalid | PathError::Empty | PathError::DotComponent => Self::Malformed,
            PathError::NotAbsolute | PathError::EscapesRoot => Self::Malformed,
        }
    }
}

impl fmt::Display for PathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "empty path",
            Self::NotAbsolute => "path is not absolute",
            Self::EscapesRoot => "path escapes the root",
            Self::TooLong => "path exceeds the length limit",
            Self::ComponentTooLong => "path component exceeds the length limit",
            Self::DotComponent => "path contains a dot component",
            Self::Invalid => "invalid path",
        })
    }
}

/// Split the first component off a relative server path.
///
/// Leading slashes are treated as the mount root and ignored. Redundant slashes between
/// components are collapsed. The returned remainder never starts with `/`. A `.` or `..` first
/// component is rejected; the caller continues with [`split_first_component`] on the remainder and
/// therefore rejects later dot components one at a time.
///
/// # Errors
///
/// Returns [`PathError`] when the path is empty, exceeds [`MAX_PATH`], has a component longer than
/// [`MAX_COMPONENT`], contains a NUL, or starts with `.` or `..`.
pub fn split_first_component(path: &str) -> Result<(&str, &str), PathError> {
    if path.len() > MAX_PATH {
        return Err(PathError::TooLong);
    }
    if path.contains('\0') {
        return Err(PathError::Invalid);
    }
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return Err(PathError::Empty);
    }
    let (first, rest) = match path.split_once('/') {
        Some((first, rest)) => (first, rest.trim_start_matches('/')),
        None => (path, ""),
    };
    if first.is_empty() {
        return Err(PathError::Empty);
    }
    if first == "." || first == ".." {
        return Err(PathError::DotComponent);
    }
    if first.len() > MAX_COMPONENT {
        return Err(PathError::ComponentTooLong);
    }
    Ok((first, rest))
}

/// Resolve `.` and `..` in an absolute path into `out`.
///
/// The result is canonical in the lexical sense: redundant slashes and `.` components are
/// removed, `..` pops the previous component, and the result never ends with a slash except at the
/// root. The returned string borrows `out` and is at most [`MAX_PATH`] bytes. `..` above `/` is an
/// error rather than being silently clamped.
///
/// # Errors
///
/// Returns [`PathError`] for relative input, escaping `..`, NUL bytes, components longer than
/// [`MAX_COMPONENT`], results longer than [`MAX_PATH`], or an output buffer that is too small.
pub fn normalize_absolute<'a>(path: &str, out: &'a mut [u8]) -> Result<&'a str, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    if path.contains('\0') {
        return Err(PathError::Invalid);
    }
    if out.is_empty() {
        return Err(PathError::TooLong);
    }
    out[0] = b'/';
    let mut length = 1usize;
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if length == 1 {
                    return Err(PathError::EscapesRoot);
                }
                let mut index = length - 1;
                while index > 0 && out[index] != b'/' {
                    index -= 1;
                }
                length = if index == 0 { 1 } else { index };
            }
            _ => {
                if component.len() > MAX_COMPONENT {
                    return Err(PathError::ComponentTooLong);
                }
                let separator = usize::from(length > 1);
                let needed = length + separator + component.len();
                if needed > MAX_PATH || needed > out.len() {
                    return Err(PathError::TooLong);
                }
                if separator == 1 {
                    out[length] = b'/';
                    length += 1;
                }
                let end = length + component.len();
                out[length..end].copy_from_slice(component.as_bytes());
                length = end;
            }
        }
    }
    core::str::from_utf8(&out[..length]).map_err(|_| PathError::Invalid)
}

/// A normalised request path plus the directory requirement the input expressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NormalizedPath<'a> {
    /// Lexically normalised absolute path.
    pub path: &'a str,
    /// The original path had a trailing slash or ended in `.`/`..`, so it must
    /// resolve to a directory.
    pub requires_dir: bool,
}

/// Normalise an absolute request path and record its directory requirement.
///
/// `vfsd` is the only layer that calls this for client paths.  A trailing
/// slash is removed by [`normalize_absolute`], so [`NormalizedPath::requires_dir`]
/// preserves the intent for the owning server.  `/` is always a directory.
pub fn normalize_request<'a>(
    path: &str,
    out: &'a mut [u8],
) -> Result<NormalizedPath<'a>, PathError> {
    let last = path.rsplit('/').next().unwrap_or("");
    let requires_dir = path.ends_with('/') || last == "." || last == "..";
    let normalized = normalize_absolute(path, out)?;
    Ok(NormalizedPath {
        path: normalized,
        requires_dir,
    })
}

#[cfg(test)]
#[path = "path_ut.rs"]
mod tests;
