// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Path joins and ordering shared by `rm`, `cp` and `mv`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;

/// Return the final component of `path`, or `path` itself when it has no separator.
pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Join a directory and one child name, rejecting separators and dot components in `name`.
///
/// This is the safety boundary for recursive deletion and copying: a directory-entry name can
/// never introduce a path above its parent.
pub fn join(dir: &str, name: &str) -> Result<String, Errno> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(Errno::EINVAL);
    }
    if dir == "/" {
        Ok(format!("/{name}"))
    } else if dir.ends_with('/') {
        Ok(format!("{dir}{name}"))
    } else {
        Ok(format!("{dir}/{name}"))
    }
}

/// Prefix an unschemed absolute path with `file:`.
///
/// `raven_rt::fs::open` forwards its argument unchanged; callers that resolved a bare path to an
/// absolute path use this helper to select the `file:` namespace.
pub fn with_file_scheme(path: &str) -> String {
    match path.find(':') {
        Some(index) if !path[..index].contains('/') => path.to_string(),
        _ => format!("file:{path}"),
    }
}

/// Sort names in byte order, which is the order the initfs and ramfs servers return.
pub fn sort_names(names: &mut [String]) {
    names.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
}

/// Filter dot names unless `all` is true.
pub fn visible_names(names: Vec<String>, all: bool) -> Vec<String> {
    if all {
        names
    } else {
        names
            .into_iter()
            .filter(|name| !name.starts_with('.'))
            .collect()
    }
}

/// Destination for `cp`/`mv` when the second operand names an existing directory.
pub fn move_into_directory(source: &str, directory: &str) -> String {
    if directory == "/" {
        format!("/{}", basename(source))
    } else if directory.ends_with('/') {
        format!("{directory}{}", basename(source))
    } else {
        format!("{directory}/{}", basename(source))
    }
}
