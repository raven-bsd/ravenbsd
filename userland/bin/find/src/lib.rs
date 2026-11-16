// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure traversal and argument handling for `find`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_proto_fs::{S_IFDIR, S_IFLNK, S_IFMT, S_IFREG};
use raven_tool_core::Errno;

/// One tree entry kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link.
    Symlink,
    /// Any other type.
    Other,
}

impl Kind {
    /// Map a POSIX mode to a find kind.
    #[must_use]
    pub fn from_mode(mode: u32) -> Self {
        match mode & S_IFMT {
            S_IFREG => Self::File,
            S_IFDIR => Self::Directory,
            S_IFLNK => Self::Symlink,
            _ => Self::Other,
        }
    }
}

/// Filesystem operations needed by the traversal.
pub trait Tree {
    /// Return the kind of one path.
    fn kind(&mut self, path: &str) -> Result<Kind, Errno>;
    /// Return the permission/mode bits of one path.
    fn mode(&mut self, path: &str) -> Result<u32, Errno>;
    /// Return the child names of one directory.
    fn children(&mut self, path: &str) -> Result<Vec<String>, Errno>;
}

/// Parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// Starting paths; default is `.`.
    pub paths: Vec<&'a str>,
    /// `-name` shell pattern.
    pub name: Option<&'a str>,
    /// `-type` filter.
    pub kind: Option<Kind>,
    /// `-perm` exact mode.
    pub perm: Option<u32>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown option.
    UnknownOption,
    /// A predicate is missing its argument.
    MissingArgument,
    /// A type or mode value is invalid.
    InvalidValue,
}

/// Parse `find [path ...] [-name pattern] [-type f|d|l] [-perm mode] [-print]`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut options = Options::default();
    let mut expression = false;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument {
            "-name" => {
                expression = true;
                options.name = Some(args.next().ok_or(UsageError::MissingArgument)?);
            }
            "-type" => {
                expression = true;
                options.kind = Some(match args.next().ok_or(UsageError::MissingArgument)? {
                    "f" => Kind::File,
                    "d" => Kind::Directory,
                    "l" => Kind::Symlink,
                    _ => return Err(UsageError::InvalidValue),
                });
            }
            "-perm" => {
                expression = true;
                let value = args.next().ok_or(UsageError::MissingArgument)?;
                options.perm =
                    Some(u32::from_str_radix(value, 8).map_err(|_| UsageError::InvalidValue)?);
            }
            "-print" => expression = true,
            value if value.starts_with('-') => return Err(UsageError::UnknownOption),
            value => {
                if expression {
                    return Err(UsageError::InvalidValue);
                }
                options.paths.push(value);
            }
        }
    }
    if options.paths.is_empty() {
        options.paths.push(".");
    }
    Ok(options)
}

/// Traverse `options.paths` and push matching display paths into `out`.
pub fn walk<T: Tree>(
    tree: &mut T,
    options: &Options<'_>,
    out: &mut Vec<String>,
) -> Result<(), Errno> {
    for path in &options.paths {
        visit(tree, options, path, out)?;
    }
    Ok(())
}

fn visit<T: Tree>(
    tree: &mut T,
    options: &Options<'_>,
    path: &str,
    out: &mut Vec<String>,
) -> Result<(), Errno> {
    let kind = tree.kind(path)?;
    if matches(tree, options, path, kind)? {
        out.push(path.to_string());
    }
    if kind == Kind::Directory {
        for name in tree.children(path)? {
            if name == "." || name == ".." {
                continue;
            }
            let child = join_display(path, &name);
            visit(tree, options, &child, out)?;
        }
    }
    Ok(())
}

fn matches<T: Tree>(
    tree: &mut T,
    options: &Options<'_>,
    path: &str,
    kind: Kind,
) -> Result<bool, Errno> {
    if let Some(pattern) = options.name
        && !glob(pattern, basename(path))
    {
        return Ok(false);
    }
    if let Some(wanted) = options.kind
        && kind != wanted
    {
        return Ok(false);
    }
    if let Some(perm) = options.perm {
        if kind == Kind::Symlink {
            return Ok(false);
        }
        if tree.mode(path)? & 0o7777 != perm {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Match a shell pattern with `*` and `?`.
#[must_use]
pub fn glob(pattern: &str, name: &str) -> bool {
    fn go(pattern: &[u8], name: &[u8]) -> bool {
        match pattern.first() {
            None => name.is_empty(),
            Some(b'*') => go(&pattern[1..], name) || (!name.is_empty() && go(pattern, &name[1..])),
            Some(b'?') => !name.is_empty() && go(&pattern[1..], &name[1..]),
            Some(byte) => name.first() == Some(byte) && go(&pattern[1..], &name[1..]),
        }
    }
    go(pattern.as_bytes(), name.as_bytes())
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn join_display(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
