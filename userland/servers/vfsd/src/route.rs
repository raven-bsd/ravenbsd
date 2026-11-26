// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-path routing helpers shared by the target handler and the host tests.
//!
//! Mount points are configuration strings, so a mount can only match the UTF-8 prefix it was
//! configured with. Everything below a mount point is a filesystem name and stays as raw bytes:
//! no component is decoded, re-encoded or length-limited here.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_vfs_core::MountTable;

/// The mount that owns an absolute byte path and the path inside it.
///
/// The longest whole-component mount prefix wins; other mounts for a path are ignored. The
/// returned suffix has no leading slash and is empty for a mount root.
pub(crate) fn mount_suffix(table: &MountTable, path: &[u8]) -> Result<(u32, Vec<u8>), Errno> {
    let mut best: Option<(u32, usize)> = None;
    for mount in table.mounts() {
        let base = mount.path.as_bytes();
        let matches = path == base
            || (path.len() > base.len()
                && path.starts_with(base)
                && (base == b"/" || path[base.len()] == b'/'));
        if matches && best.is_none_or(|(_, length)| base.len() > length) {
            best = Some((mount.id, base.len()));
        }
    }
    let (id, length) = best.ok_or(Errno::ENOENT)?;
    let suffix = if path.len() == length {
        Vec::new()
    } else if length == 1 {
        path[1..].to_vec()
    } else {
        path[length + 1..].to_vec()
    };
    Ok((id, suffix))
}

/// The parent prefix of a mount-relative path; empty for a top-level name.
pub(crate) fn parent_bytes(suffix: &[u8]) -> &[u8] {
    let trimmed = suffix.strip_suffix(b"/").unwrap_or(suffix);
    match trimmed.iter().rposition(|byte| *byte == b'/') {
        Some(index) => &trimmed[..index],
        None => b"",
    }
}

/// Every non-empty directory prefix of a mount-relative path, deepest last.
pub(crate) fn component_prefixes(suffix: &[u8]) -> Vec<&[u8]> {
    let trimmed = suffix.strip_suffix(b"/").unwrap_or(suffix);
    let mut prefixes = Vec::new();
    for (index, byte) in trimmed.iter().enumerate() {
        if *byte == b'/' && index > 0 {
            prefixes.push(&trimmed[..index]);
        }
    }
    if !trimmed.is_empty() {
        prefixes.push(trimmed);
    }
    prefixes
}

/// The path below `root` for an OPEN path that already carries the root prefix.
///
/// The kernel passes the per-process root as a prefix of the open path and its byte length in
/// `Ctx::root_len`. The walker resolves absolute payloads from the root, so the prefix is removed
/// first; a path outside the root cannot be opened.
pub(crate) fn relative_to_root<'a>(root: &str, path: &'a str) -> Result<&'a str, Errno> {
    if root == "/" {
        return Ok(path);
    }
    if path == root {
        return Ok("");
    }
    match path.strip_prefix(root) {
        Some(rest) if rest.starts_with('/') => Ok(rest),
        _ => Err(Errno::ENOENT),
    }
}

#[cfg(test)]
#[path = "route_ut.rs"]
mod tests;
