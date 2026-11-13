// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The lexical resolver of doc 11 section 3.4 (algorithm 5a-5f).

use alloc::vec::Vec;

/// Fixed limits used by [`resolve`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// `PATH_MAX`: rejecting input longer than this before parsing.
    pub path_max: usize,
    /// `NAME_MAX`: one component.
    pub component_max: usize,
    /// The effective transport limit of the scheme path (255 today, PG-40 widens it).
    pub transport_max: usize,
}

/// Defaults fixed by doc 11 section 3.2: 4096 / 255 / 255.
pub const DEFAULT_LIMITS: Limits = Limits {
    path_max: 4096,
    component_max: 255,
    transport_max: 255,
};

/// Why the lexical resolver rejected a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathReason {
    /// An empty path without `AT_EMPTY_PATH`.
    Empty,
    /// The whole path exceeds `PATH_MAX` or the transport limit.
    TooLong,
    /// One component exceeds `NAME_MAX`.
    ComponentTooLong,
    /// The path contains a NUL byte.
    Nul,
    /// A relative path was given with a non-absolute base.
    NotAbsolute,
}

/// A resolved absolute path and the directory requirement of the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// Absolute, lexically normalised path.
    pub abs: Vec<u8>,
    /// The input ended in `/`, `.` or `..`, so the final object must be a directory.
    pub must_be_dir: bool,
}

/// Resolve `path` against the absolute `base`.
///
/// An empty path is `ENOENT` unless `empty_ok` (`AT_EMPTY_PATH`), in which case the base itself is
/// returned. Absolute paths ignore the base. `..` at the root stays at the root, which is the
/// POSIX rule and keeps `/..` a directory.
pub fn resolve(
    base: &[u8],
    path: &[u8],
    empty_ok: bool,
    limits: &Limits,
) -> Result<Resolved, PathReason> {
    if path.is_empty() {
        if !empty_ok {
            return Err(PathReason::Empty);
        }
        let abs = normalize_absolute(base, limits)?;
        return Ok(Resolved {
            abs,
            must_be_dir: true,
        });
    }
    if path.len() > limits.path_max {
        return Err(PathReason::TooLong);
    }
    if path.contains(&0) || base.contains(&0) {
        return Err(PathReason::Nul);
    }
    if !base.starts_with(b"/") {
        return Err(PathReason::NotAbsolute);
    }

    let last = last_component(path);
    let must_be_dir = path.ends_with(b"/") || last == b"." || last == b"..";

    let mut absolute = Vec::with_capacity(limits.path_max.min(path.len() + base.len() + 1));
    if path.starts_with(b"/") {
        absolute.push(b'/');
    } else {
        absolute.extend_from_slice(base);
        if !absolute.ends_with(b"/") {
            absolute.push(b'/');
        }
    }

    for component in path.split(|&byte| byte == b'/') {
        match component {
            b"" | b"." => {}
            b".." => {
                if absolute.len() > 1 {
                    if absolute.ends_with(b"/") {
                        absolute.pop();
                    }
                    while let Some(byte) = absolute.pop() {
                        if byte == b'/' {
                            break;
                        }
                    }
                    if absolute.len() > 1 && absolute.ends_with(b"/") {
                        absolute.pop();
                    }
                    if absolute.is_empty() {
                        absolute.push(b'/');
                    }
                }
            }
            _ => {
                if component.len() > limits.component_max {
                    return Err(PathReason::ComponentTooLong);
                }
                if absolute.len() > 1 && !absolute.ends_with(b"/") {
                    absolute.push(b'/');
                }
                absolute.extend_from_slice(component);
            }
        }
    }
    if absolute.is_empty() {
        absolute.push(b'/');
    }
    while absolute.len() > 1 && absolute.ends_with(b"/") {
        absolute.pop();
    }
    if absolute.len() > limits.path_max || absolute.len() > limits.transport_max {
        return Err(PathReason::TooLong);
    }
    Ok(Resolved {
        abs: absolute,
        must_be_dir,
    })
}

fn normalize_absolute(base: &[u8], limits: &Limits) -> Result<Vec<u8>, PathReason> {
    let resolved = resolve(b"/", base, false, limits)?;
    if resolved.abs.len() > limits.transport_max {
        return Err(PathReason::TooLong);
    }
    Ok(resolved.abs)
}

fn last_component(path: &[u8]) -> &[u8] {
    match path.rsplit(|&byte| byte == b'/').next() {
        Some(last) => last,
        None => path,
    }
}

/// Prefix a resolved absolute path with `file:` exactly once (A-sys-7).
pub fn file_path(abs: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(abs.len() + 5);
    out.extend_from_slice(b"file:");
    out.extend_from_slice(abs);
    out
}
