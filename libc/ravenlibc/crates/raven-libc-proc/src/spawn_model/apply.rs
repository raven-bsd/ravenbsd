// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The parent-side scratch fd-table applier.
//!
//! Nothing runs in the child before `main` on Raven, so spawn file actions are applied to a copy
//! of the parent's fd table and sent as the `fdmap` of the `SPAWN` request.

use alloc::vec::Vec;

use crate::errors::{EBADF, ENOSYS, ENOTDIR, Errno};
use crate::spawn_model::api::{FileAction, FileActions};
use crate::wire::FdMapEntry;

/// One inherited descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FdEntry {
    /// Descriptor number.
    pub fd: i32,
    /// Scheme handle.
    pub handle: u32,
    /// Whether the descriptor closes at exec.
    pub cloexec: bool,
}

/// The effects the applier needs from the file layer.
pub trait SpawnEnv {
    /// Open `path` in the parent and return its handle.
    fn open(&mut self, path: &[u8], oflag: i32, mode: u32) -> Result<u32, Errno>;
    /// Whether `handle` names a directory.
    fn is_dir(&mut self, handle: u32) -> Result<bool, Errno>;
    /// Close a temporary handle.
    fn close_temp(&mut self, handle: u32);
    /// Return the path of `handle`, when the fd table carries one.
    fn fd_path(&mut self, handle: u32) -> Option<Vec<u8>>;
}

/// The result of applying a file-action list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scratch {
    /// Surviving descriptors, sorted by number.
    pub fdmap: Vec<FdMapEntry>,
    /// The scratch current directory.
    pub cwd: Vec<u8>,
    /// Handles opened by `Open`; the caller closes them after the reply.
    pub temp_handles: Vec<u32>,
    /// Remembered controlling-terminal descriptor.
    pub tcsetpgrp_fd: i32,
}

/// Apply `fa` to a copy of `table`.
pub fn apply_actions(
    table: &[FdEntry],
    cwd: &[u8],
    cloexec_default: bool,
    fa: &FileActions,
    env: &mut dyn SpawnEnv,
) -> Result<Scratch, Errno> {
    let mut entries = table.to_vec();
    if cloexec_default {
        for entry in &mut entries {
            entry.cloexec = true;
        }
    }
    let mut scratch = Scratch {
        fdmap: Vec::new(),
        cwd: if cwd.is_empty() {
            b"/".to_vec()
        } else {
            cwd.to_vec()
        },
        temp_handles: Vec::new(),
        tcsetpgrp_fd: -1,
    };
    let result = apply_inner(&mut entries, &mut scratch, fa, env);
    if result.is_err() {
        for handle in &scratch.temp_handles {
            env.close_temp(*handle);
        }
        return result.map(|()| scratch);
    }
    entries.retain(|entry| !entry.cloexec);
    entries.sort_by_key(|entry| entry.fd);
    scratch.fdmap = entries
        .iter()
        .map(|entry| FdMapEntry {
            fd: entry.fd,
            handle: entry.handle,
        })
        .collect();
    Ok(scratch)
}

fn apply_inner(
    entries: &mut Vec<FdEntry>,
    scratch: &mut Scratch,
    fa: &FileActions,
    env: &mut dyn SpawnEnv,
) -> Result<(), Errno> {
    for action in fa.actions() {
        match action {
            FileAction::Open {
                fd,
                path,
                oflag,
                mode,
            } => {
                let handle = env.open(path, *oflag, *mode)?;
                scratch.temp_handles.push(handle);
                set_entry(
                    entries,
                    FdEntry {
                        fd: *fd,
                        handle,
                        cloexec: false,
                    },
                );
            }
            FileAction::Dup2 { fd, newfd } => {
                let source = entries
                    .iter()
                    .find(|entry| entry.fd == *fd)
                    .copied()
                    .ok_or(EBADF)?;
                set_entry(
                    entries,
                    FdEntry {
                        fd: *newfd,
                        handle: source.handle,
                        cloexec: false,
                    },
                );
            }
            FileAction::Close { fd } => {
                entries.retain(|entry| entry.fd != *fd);
            }
            FileAction::Chdir { path } => {
                scratch.cwd = resolve(&scratch.cwd, path);
            }
            FileAction::Fchdir { fd } => {
                let entry = entries
                    .iter()
                    .find(|entry| entry.fd == *fd)
                    .copied()
                    .ok_or(EBADF)?;
                if !env.is_dir(entry.handle)? {
                    return Err(ENOTDIR);
                }
                let path = env.fd_path(entry.handle).ok_or(ENOSYS)?;
                scratch.cwd = resolve(&scratch.cwd, &path);
            }
            FileAction::CloseFrom { from } => {
                entries.retain(|entry| entry.fd < *from);
            }
            FileAction::TcSetPgrp { fd } => {
                scratch.tcsetpgrp_fd = *fd;
            }
        }
    }
    Ok(())
}

fn set_entry(entries: &mut Vec<FdEntry>, value: FdEntry) {
    if let Some(entry) = entries.iter_mut().find(|entry| entry.fd == value.fd) {
        *entry = value;
    } else {
        entries.push(value);
    }
}

/// Resolve `path` against `cwd`, lexically, keeping the result absolute.
#[must_use]
pub fn resolve(cwd: &[u8], path: &[u8]) -> Vec<u8> {
    let mut components: Vec<Vec<u8>> = Vec::new();
    if !path.starts_with(b"/") {
        for component in cwd.split(|byte| *byte == b'/') {
            push_component(&mut components, component);
        }
    }
    for component in path.split(|byte| *byte == b'/') {
        push_component(&mut components, component);
    }
    let mut result = Vec::new();
    result.push(b'/');
    for (index, component) in components.iter().enumerate() {
        if index != 0 {
            result.push(b'/');
        }
        result.extend_from_slice(component);
    }
    result
}

fn push_component(components: &mut Vec<Vec<u8>>, component: &[u8]) {
    if component.is_empty() || component == b"." {
        return;
    }
    if component == b".." {
        components.pop();
        return;
    }
    components.push(component.to_vec());
}

#[cfg(test)]
#[path = "apply_ut.rs"]
mod tests;
