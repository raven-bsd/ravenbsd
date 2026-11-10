// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Allocation-free path helpers shared by the servers.
//!
//! `vfsd` normalises absolute client paths before routing them. Filesystem servers receive paths
//! relative to their mount root and walk them one component at a time, rejecting `.` and `..` so
//! no client can escape the server's root with a relative suffix.

use core::fmt;

use crate::{Errno, ProtoError};

/// Maximum path length in bytes, including a leading `/` for absolute paths.
pub const MAX_PATH: usize = 255;
/// Maximum single component length in bytes.
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

#[cfg(test)]
mod tests {
    use super::*;

    const CASES: &[(&str, Result<&str, PathError>)] = &[
        ("/", Ok("/")),
        ("/a", Ok("/a")),
        ("/a/", Ok("/a")),
        ("/a//b", Ok("/a/b")),
        ("/a/./b", Ok("/a/b")),
        ("/./a", Ok("/a")),
        ("/a/b/..", Ok("/a")),
        ("/a/b/../..", Ok("/")),
        ("/a/b/../../c", Ok("/c")),
        ("/a/../b", Ok("/b")),
        ("/a/../../b", Err(PathError::EscapesRoot)),
        ("/..", Err(PathError::EscapesRoot)),
        ("/../", Err(PathError::EscapesRoot)),
        ("", Err(PathError::NotAbsolute)),
        ("a", Err(PathError::NotAbsolute)),
        ("a/b", Err(PathError::NotAbsolute)),
        ("/a/...", Ok("/a/...")),
        ("/a/.hidden", Ok("/a/.hidden")),
        ("/a/..hidden", Ok("/a/..hidden")),
        ("/a/b/./c", Ok("/a/b/c")),
        ("/a/b/c/../../d", Ok("/a/d")),
        ("/a/b/c/../../../d", Ok("/d")),
        ("/a/b/c/../../../../d", Err(PathError::EscapesRoot)),
        ("//a", Ok("/a")),
        ("///", Ok("/")),
        ("/a///b///", Ok("/a/b")),
        ("/a/./././b", Ok("/a/b")),
        ("/a/b/./../c", Ok("/a/c")),
        ("/a/b/.././c", Ok("/a/c")),
        ("/.", Ok("/")),
        ("/./", Ok("/")),
        ("/.//", Ok("/")),
        ("/a/.", Ok("/a")),
        ("/a/..", Ok("/")),
        ("/a/b/../..", Ok("/")),
        ("/x/../y/../z", Ok("/z")),
        ("/x/y/../z/..", Ok("/x")),
        ("/x/y/../z/../w", Ok("/x/w")),
        ("/A/B", Ok("/A/B")),
        ("/a b/c", Ok("/a b/c")),
        ("/é/ß", Ok("/é/ß")),
        ("/a/-b", Ok("/a/-b")),
        ("/a/_b", Ok("/a/_b")),
        ("/a/b-c_d.e", Ok("/a/b-c_d.e")),
        ("/a/b/..", Ok("/a")),
        ("/a/b/../c", Ok("/a/c")),
        ("/a/b/c/d/e/f", Ok("/a/b/c/d/e/f")),
        ("/a/b/c/d/e/..", Ok("/a/b/c/d")),
        ("/a/b/c/d/e/../..", Ok("/a/b/c")),
        ("/a/b/c/d/e/../../..", Ok("/a/b")),
        ("/a/b/c/d/e/../../../..", Ok("/a")),
        ("/a/b/c/d/e/../../../../..", Ok("/")),
        ("/a/b/c/d/e/../../../../../..", Err(PathError::EscapesRoot)),
        (
            "/a/b/c/d/e/../../../../../../..",
            Err(PathError::EscapesRoot),
        ),
        ("/a/b/c/d/e/f/../g", Ok("/a/b/c/d/e/g")),
        ("/a/b/c/d/e/f/../../g", Ok("/a/b/c/d/g")),
        ("/a/b/c/d/e/f/../../../g", Ok("/a/b/c/g")),
        ("/a/b/c/d/e/f/../../../../g", Ok("/a/b/g")),
        ("/a/b/c/d/e/f/../../../../../g", Ok("/a/g")),
        ("/a/b/c/d/e/f/../../../../../../g", Ok("/g")),
    ];

    #[test]
    fn normalize_table_has_sixty_cases() {
        assert_eq!(CASES.len(), 60);
        let mut buffer = [0u8; MAX_PATH];
        for (input, expected) in CASES {
            let actual = normalize_absolute(input, &mut buffer);
            assert_eq!(actual, *expected, "input {input:?}");
        }
    }

    #[test]
    fn normalize_rejects_nul() {
        let mut buffer = [0u8; MAX_PATH];
        assert_eq!(
            normalize_absolute("/a\0b", &mut buffer),
            Err(PathError::Invalid)
        );
    }

    #[test]
    fn normalize_accepts_exact_max_path() {
        let input = format!("/{}", "a".repeat(MAX_PATH - 1));
        let mut buffer = [0u8; MAX_PATH];
        assert_eq!(
            normalize_absolute(&input, &mut buffer).unwrap().len(),
            MAX_PATH
        );
    }

    #[test]
    fn normalize_rejects_long_path() {
        let input = format!("/{}", "a".repeat(MAX_PATH));
        let mut buffer = [0u8; MAX_PATH + 4];
        assert_eq!(
            normalize_absolute(&input, &mut buffer),
            Err(PathError::TooLong)
        );
    }

    #[test]
    fn normalize_rejects_long_component() {
        let input = format!("/{}", "a".repeat(MAX_COMPONENT + 1));
        let mut buffer = [0u8; MAX_PATH * 2];
        assert_eq!(
            normalize_absolute(&input, &mut buffer),
            Err(PathError::ComponentTooLong)
        );
    }

    #[test]
    fn normalize_rejects_small_output_buffer() {
        let mut buffer = [0u8; 3];
        assert_eq!(
            normalize_absolute("/abcd", &mut buffer),
            Err(PathError::TooLong)
        );
    }

    #[test]
    fn split_extracts_first_component() {
        assert_eq!(split_first_component("a/b"), Ok(("a", "b")));
        assert_eq!(split_first_component("a"), Ok(("a", "")));
        assert_eq!(split_first_component("/a/b"), Ok(("a", "b")));
        assert_eq!(split_first_component("//a///b"), Ok(("a", "b")));
        assert_eq!(split_first_component("a/"), Ok(("a", "")));
        assert_eq!(split_first_component("a//b"), Ok(("a", "b")));
        assert_eq!(split_first_component("é/ß"), Ok(("é", "ß")));
    }

    #[test]
    fn split_rejects_empty_and_dot_components() {
        assert_eq!(split_first_component(""), Err(PathError::Empty));
        assert_eq!(split_first_component("/"), Err(PathError::Empty));
        assert_eq!(split_first_component("."), Err(PathError::DotComponent));
        assert_eq!(split_first_component(".."), Err(PathError::DotComponent));
        assert_eq!(split_first_component("/./a"), Err(PathError::DotComponent));
        assert_eq!(split_first_component("/../a"), Err(PathError::DotComponent));
    }

    #[test]
    fn split_bounds_components_and_total_length() {
        assert_eq!(split_first_component("/a\0b"), Err(PathError::Invalid));
        let long_component = "a".repeat(MAX_COMPONENT + 1);
        assert_eq!(
            split_first_component(&long_component),
            Err(PathError::TooLong)
        );
        let long_path = format!("/{}", "a".repeat(MAX_PATH));
        assert_eq!(split_first_component(&long_path), Err(PathError::TooLong));
    }

    #[test]
    fn path_errors_map_to_errno_and_proto_errors() {
        assert_eq!(PathError::TooLong.errno(), Errno::ENAMETOOLONG);
        assert_eq!(PathError::Invalid.errno(), Errno::EINVAL);
        assert_eq!(
            ProtoError::from(PathError::TooLong),
            ProtoError::LengthOutOfRange
        );
        assert_eq!(ProtoError::from(PathError::Empty), ProtoError::Malformed);
    }
}
