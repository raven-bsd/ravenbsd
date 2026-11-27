// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `realpath(3)`: canonicalise a path, following symlinks.
//!
//! The platform owns the filesystem calls; this file owns the POSIX path walk.
//! A missing component is `ENOENT`, a symlink loop is `ELOOP`, a trailing slash
//! on a non-directory is `ENOTDIR`, and a result that does not fit `PATH_MAX` is
//! `ENAMETOOLONG`. The `PathOps` seam keeps the walk host-testable.
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use alloc::collections::VecDeque;
use alloc::vec::Vec;
#[cfg(target_os = "none")]
use core::ffi::c_char;

use raven_libc_abi::limits::{PATH_MAX, SYMLOOP_MAX};
use raven_libc_abi::stat::{S_IFDIR, S_IFMT};
use raven_libc_abi::structs::Stat;
use raven_libc_platform::Errno;

/// The filesystem calls the canonicaliser needs.
pub(crate) trait PathOps {
    /// Read the target of a symlink, or `EINVAL` when `path` is not one.
    fn read_link(&mut self, path: &[u8], out: &mut [u8]) -> Result<usize, Errno>;
    /// Stat `path`, following no symlink (the caller already checked with `read_link`).
    fn stat(&mut self, path: &[u8]) -> Result<Stat, Errno>;
    /// The process working directory.
    fn getcwd(&mut self) -> Result<Vec<u8>, Errno>;
}

/// The real platform implementation.
#[cfg(target_os = "none")]
struct RealPathOps;

#[cfg(target_os = "none")]
impl PathOps for RealPathOps {
    fn read_link(&mut self, path: &[u8], out: &mut [u8]) -> Result<usize, Errno> {
        crate::ffi::platform::global().readlink(path, out)
    }

    fn stat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        crate::ffi::platform::global().stat_at(raven_libc_abi::fcntl::AT_FDCWD, path, 0)
    }

    fn getcwd(&mut self) -> Result<Vec<u8>, Errno> {
        crate::ffi::platform::global()
            .getcwd(None, 0)?
            .ok_or(Errno::EIO)
    }
}

fn components(path: &[u8]) -> VecDeque<Vec<u8>> {
    let mut output = VecDeque::new();
    for part in path.split(|byte| *byte == b'/') {
        if !part.is_empty() {
            output.push_back(part.to_vec());
        }
    }
    output
}

fn candidate(work: &[Vec<u8>], component: &[u8]) -> Result<Vec<u8>, Errno> {
    let mut output = Vec::new();
    for part in work {
        output.push(b'/');
        output.extend_from_slice(part);
    }
    output.push(b'/');
    output.extend_from_slice(component);
    if output.len() >= PATH_MAX as usize {
        return Err(Errno::ENAMETOOLONG);
    }
    Ok(output)
}

fn finish(work: &[Vec<u8>]) -> Result<Vec<u8>, Errno> {
    if work.is_empty() {
        return Ok(b"/".to_vec());
    }
    let mut output = Vec::new();
    for part in work {
        output.push(b'/');
        output.extend_from_slice(part);
    }
    if output.len() >= PATH_MAX as usize {
        return Err(Errno::ENAMETOOLONG);
    }
    Ok(output)
}

/// Canonicalise `input` against `ops`.
pub(crate) fn canonicalize(ops: &mut impl PathOps, input: &[u8]) -> Result<Vec<u8>, Errno> {
    if input.is_empty() {
        return Err(Errno::ENOENT);
    }
    let mut work = if input.first() == Some(&b'/') {
        Vec::new()
    } else {
        let cwd = ops.getcwd()?;
        if cwd.first() != Some(&b'/') {
            return Err(Errno::EINVAL);
        }
        components(&cwd).into_iter().collect()
    };
    let mut queue = components(input);
    let trailing_slash = input.ends_with(b"/") && input.iter().any(|byte| *byte != b'/');
    let mut links = 0usize;
    while let Some(component) = queue.pop_front() {
        match component.as_slice() {
            b"" | b"." => continue,
            b".." => {
                work.pop();
                continue;
            }
            _ => {}
        }
        let path = candidate(&work, &component)?;
        let mut target = [0u8; PATH_MAX as usize];
        match ops.read_link(&path, &mut target) {
            Ok(length) => {
                links = links.checked_add(1).ok_or(Errno::ELOOP)?;
                if links > SYMLOOP_MAX as usize {
                    return Err(Errno::ELOOP);
                }
                if length == 0 {
                    return Err(Errno::ENOENT);
                }
                let target = &target[..length];
                if target.first() == Some(&b'/') {
                    work.clear();
                }
                let mut parts = components(target);
                while let Some(part) = parts.pop_back() {
                    queue.push_front(part);
                }
            }
            Err(Errno::EINVAL) => {
                ops.stat(&path)?;
                work.push(component);
            }
            Err(error) => return Err(error),
        }
    }
    let result = finish(&work)?;
    if trailing_slash {
        let stat = ops.stat(&result)?;
        if u32::from(stat.st_mode) & S_IFMT != S_IFDIR {
            return Err(Errno::ENOTDIR);
        }
    }
    Ok(result)
}

/// `realpath(path, resolved_path)`.
///
/// # Safety
/// `path` is a NUL-terminated string; `resolved` is NULL or a writable `PATH_MAX`
/// buffer. The resolved path is allocated with `malloc` when `resolved` is NULL.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn realpath(path: *const c_char, resolved: *mut c_char) -> *mut c_char {
    if path.is_null() {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's NUL-terminated path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    let mut ops = RealPathOps;
    match canonicalize(&mut ops, path) {
        Ok(result) => {
            if resolved.is_null() {
                // SAFETY: the shell allocator returns a writable NUL-terminated copy.
                unsafe { crate::platform::copy_c_path(&result) }
            } else if result.len() + 1 > PATH_MAX as usize {
                crate::ffi::errno::set(raven_libc_abi::errno::Errno(
                    raven_libc_abi::errno::ENAMETOOLONG as u16,
                ));
                core::ptr::null_mut()
            } else {
                // SAFETY: the caller's buffer is PATH_MAX writable bytes; copy, not
                // copy_nonoverlapping because `resolved` may alias the input.
                unsafe {
                    core::ptr::copy(result.as_ptr(), resolved.cast::<u8>(), result.len());
                    *resolved.add(result.len()) = 0;
                }
                resolved
            }
        }
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            core::ptr::null_mut()
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "realpath_ut.rs"]
mod tests;
