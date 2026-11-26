// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Component-by-component rooted path traversal.
//!
//! `vfsd` resolves every path one component at a time against the caller's root, the way
//! `namei` does in a kernel. Lexical `..` collapsing is wrong here: a `..` after a symlink must
//! pop the directory the link resolved to, not the directory that was written. A symlink
//! replaces only the component it occupies; the unprocessed remainder of the path stays pending.
//!
//! The walk is parameterised over [`Namespace`] so the algorithm runs under `cargo nextest`
//! without a target kernel.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use raven_abi::errno::Errno;

/// Symlink expansions allowed for one lookup.
///
/// FreeBSD `sys/sys/param.h` defines `MAXSYMLINKS 32`; `namei_follow_link` returns `ELOOP`
/// when `ni_loopcnt++ >= MAXSYMLINKS`.
pub(crate) const MAX_SYMLINKS: usize = 32;

/// What a path names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A directory.
    Directory,
    /// A symbolic link.
    Symlink,
    /// Any other object.
    Other,
}

/// The operations a walk needs from the namespace it resolves in.
pub(crate) trait Namespace {
    /// Stat `path` (absolute, no NUL, at most the caller's path limit) without following a
    /// final symbolic link.
    fn lstat(&mut self, path: &[u8]) -> Result<Kind, Errno>;

    /// Return the target bytes of the symbolic link at `path`.
    fn readlink(&mut self, path: &[u8]) -> Result<Vec<u8>, Errno>;
}

fn push_components(queue: &mut VecDeque<Vec<u8>>, text: &[u8]) {
    let mut reverse: Vec<Vec<u8>> = text
        .split(|byte| *byte == b'/')
        .filter(|component| !component.is_empty())
        .map(<[u8]>::to_vec)
        .collect();
    reverse.reverse();
    for component in reverse {
        queue.push_front(component);
    }
}

fn join(parent: &[u8], component: &[u8]) -> Vec<u8> {
    let mut path = parent.to_vec();
    if path.last() != Some(&b'/') {
        path.push(b'/');
    }
    path.extend_from_slice(component);
    path
}

fn parent_of(root: &[u8], current: &[u8]) -> Vec<u8> {
    if current == root {
        return current.to_vec();
    }
    match current.iter().rposition(|byte| *byte == b'/') {
        Some(0) | None => b"/".to_vec(),
        Some(index) => current[..index].to_vec(),
    }
}

/// Resolve `path` under `root`, starting from `base`.
///
/// An absolute `path` starts at `root`; a relative one starts at `base`. `..` never climbs
/// above `root`. A symlink target that starts with `/` is resolved from `root`; any other target
/// is resolved from the directory that contains the link. The component that follows a symlink
/// (and everything after it) stays pending and is processed after the link target.
///
/// When `follow_final` is false the last component is not expanded even when it is a symlink.
/// A missing last component is returned as a normal path so the caller can create it.
///
/// # Errors
///
/// Returns `ENAMETOOLONG` when the resolved path exceeds `max_path`, `ELOOP` when more than
/// [`MAX_SYMLINKS`] links are followed, `ENOENT` for a missing intermediate component,
/// `ENOTDIR` when a component continues below a non-directory, and whatever the namespace
/// reports for a failed stat or readlink.
pub(crate) fn resolve(
    namespace: &mut impl Namespace,
    root: &[u8],
    base: &[u8],
    path: &[u8],
    follow_final: bool,
    max_path: usize,
) -> Result<Vec<u8>, Errno> {
    if root.first() != Some(&b'/') || base.first() != Some(&b'/') {
        return Err(Errno::EINVAL);
    }
    if path.contains(&0) || path.len() > max_path {
        return Err(Errno::ENAMETOOLONG);
    }
    if base != root
        && !(root == b"/")
        && !(base.starts_with(root) && base.get(root.len()) == Some(&b'/'))
    {
        return Err(Errno::ENOENT);
    }

    let (mut current, text) = match path.first() {
        Some(b'/') => (root.to_vec(), &path[1..]),
        _ => (base.to_vec(), path),
    };
    let mut pending = VecDeque::new();
    push_components(&mut pending, text);
    let mut links = 0usize;

    while let Some(component) = pending.pop_front() {
        if component == b"." {
            continue;
        }
        if component == b".." {
            current = parent_of(root, &current);
            continue;
        }
        let candidate = join(&current, &component);
        if candidate.len() > max_path {
            return Err(Errno::ENAMETOOLONG);
        }
        let last = pending.is_empty();
        match namespace.lstat(&candidate) {
            Ok(Kind::Symlink) => {
                if last && !follow_final {
                    return Ok(candidate);
                }
                links += 1;
                if links > MAX_SYMLINKS {
                    return Err(Errno::ELOOP);
                }
                let target = namespace.readlink(&candidate)?;
                if target.is_empty() {
                    return Err(Errno::ENOENT);
                }
                if target.len() > max_path {
                    return Err(Errno::ENAMETOOLONG);
                }
                if target.first() == Some(&b'/') {
                    current = root.to_vec();
                    push_components(&mut pending, &target[1..]);
                } else {
                    push_components(&mut pending, &target);
                }
            }
            Ok(Kind::Directory) => current = candidate,
            Ok(Kind::Other) => {
                if last {
                    current = candidate;
                } else {
                    return Err(Errno::ENOTDIR);
                }
            }
            Err(Errno::ENOENT) if last => return Ok(candidate),
            Err(error) => return Err(error),
        }
    }
    Ok(current)
}

#[cfg(test)]
#[path = "walk_ut.rs"]
mod tests;
