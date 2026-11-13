// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! An independent reference interpreter for the spawn file-action table.
//!
//! The semantics are written from the POSIX file-action text, not from `apply.rs`: a
//! `BTreeMap<i32, (u32, bool)>`, the same five actions and a separately written lexical path
//! resolver. Two thousand seeded action lists are compared with `apply_actions`.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::unwrap_used
)] // a test uses fixed vectors and fails by panicking

use std::collections::BTreeMap;

use raven_libc_proc::errors::{EBADF, ENOENT, ENOSYS, ENOTDIR, Errno};
use raven_libc_proc::spawn_model::{FdEntry, FileAction, FileActions, SpawnEnv, apply_actions};
use raven_libc_proc::wire::FdMapEntry;

#[derive(Debug, Default)]
struct RefScratch {
    fdmap: Vec<FdMapEntry>,
    cwd: Vec<u8>,
    temp_handles: Vec<u32>,
    tcsetpgrp_fd: i32,
}

struct ScriptedEnv {
    next_handle: u32,
    closed: Vec<u32>,
}

impl ScriptedEnv {
    fn new() -> Self {
        Self {
            next_handle: 1000,
            closed: Vec::new(),
        }
    }
}

impl SpawnEnv for ScriptedEnv {
    fn open(&mut self, path: &[u8], _oflag: i32, _mode: u32) -> Result<u32, Errno> {
        if path.windows(3).any(|window| window == b"bad") {
            return Err(ENOENT);
        }
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        Ok(handle)
    }

    fn is_dir(&mut self, handle: u32) -> Result<bool, Errno> {
        Ok(!handle.is_multiple_of(3))
    }

    fn close_temp(&mut self, handle: u32) {
        self.closed.push(handle);
    }

    fn fd_path(&mut self, handle: u32) -> Option<Vec<u8>> {
        if handle.is_multiple_of(5) {
            None
        } else {
            Some(format!("/dir/{handle}").into_bytes())
        }
    }
}

fn close_temps(env: &mut ScriptedEnv, temps: &[u32]) {
    for handle in temps {
        env.close_temp(*handle);
    }
}

fn ref_resolve(cwd: &[u8], path: &[u8]) -> Vec<u8> {
    let mut stack: Vec<Vec<u8>> = Vec::new();
    if !path.starts_with(b"/") {
        for part in cwd.split(|byte| *byte == b'/') {
            ref_push(&mut stack, part);
        }
    }
    for part in path.split(|byte| *byte == b'/') {
        ref_push(&mut stack, part);
    }
    let mut out = vec![b'/'];
    for (index, part) in stack.iter().enumerate() {
        if index != 0 {
            out.push(b'/');
        }
        out.extend_from_slice(part);
    }
    out
}

fn ref_push(stack: &mut Vec<Vec<u8>>, part: &[u8]) {
    if part.is_empty() || part == b"." {
        return;
    }
    if part == b".." {
        stack.pop();
        return;
    }
    stack.push(part.to_vec());
}

fn reference(
    table: &[FdEntry],
    cwd: &[u8],
    cloexec_default: bool,
    actions: &FileActions,
    env: &mut ScriptedEnv,
) -> Result<RefScratch, Errno> {
    let mut map: BTreeMap<i32, (u32, bool)> = BTreeMap::new();
    for entry in table {
        map.insert(entry.fd, (entry.handle, entry.cloexec || cloexec_default));
    }
    let mut cwd = if cwd.is_empty() {
        b"/".to_vec()
    } else {
        cwd.to_vec()
    };
    let mut temp_handles = Vec::new();
    let mut tcsetpgrp_fd = -1;
    for action in actions.actions() {
        match action {
            FileAction::Open { fd, path, .. } => match env.open(path, 0, 0) {
                Ok(handle) => {
                    temp_handles.push(handle);
                    map.insert(*fd, (handle, false));
                }
                Err(error) => {
                    close_temps(env, &temp_handles);
                    return Err(error);
                }
            },
            FileAction::Dup2 { fd, newfd } => {
                let Some((handle, _)) = map.get(fd).copied() else {
                    close_temps(env, &temp_handles);
                    return Err(EBADF);
                };
                map.insert(*newfd, (handle, false));
            }
            FileAction::Close { fd } => {
                map.remove(fd);
            }
            FileAction::Chdir { path } => {
                cwd = ref_resolve(&cwd, path);
            }
            FileAction::Fchdir { fd } => {
                let Some((handle, _)) = map.get(fd).copied() else {
                    close_temps(env, &temp_handles);
                    return Err(EBADF);
                };
                match env.is_dir(handle) {
                    Ok(true) => {}
                    Ok(false) => {
                        close_temps(env, &temp_handles);
                        return Err(ENOTDIR);
                    }
                    Err(error) => {
                        close_temps(env, &temp_handles);
                        return Err(error);
                    }
                }
                let Some(path) = env.fd_path(handle) else {
                    close_temps(env, &temp_handles);
                    return Err(ENOSYS);
                };
                cwd = ref_resolve(&cwd, &path);
            }
            FileAction::CloseFrom { from } => {
                map.retain(|fd, _| fd < from);
            }
            FileAction::TcSetPgrp { fd } => {
                tcsetpgrp_fd = *fd;
            }
        }
    }
    map.retain(|_, (_, cloexec)| !*cloexec);
    let fdmap = map
        .iter()
        .map(|(fd, (handle, _))| FdMapEntry {
            fd: *fd,
            handle: *handle,
        })
        .collect();
    Ok(RefScratch {
        fdmap,
        cwd,
        temp_handles,
        tcsetpgrp_fd,
    })
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }
}

fn build_actions(rng: &mut Rng) -> FileActions {
    let mut actions = FileActions::new();
    let count = rng.next() % 12;
    for _ in 0..count {
        match rng.next() % 7 {
            0 => {
                let fd = (rng.next() % 12) as i32;
                let path = if rng.next().is_multiple_of(7) {
                    b"bad".as_slice()
                } else {
                    b"/tmp/x".as_slice()
                };
                let _ = actions.add_open(fd, path, 0, 0);
            }
            1 => {
                let fd = (rng.next() % 12) as i32;
                let newfd = (rng.next() % 12) as i32;
                let _ = actions.add_dup2(fd, newfd);
            }
            2 => {
                let fd = (rng.next() % 12) as i32;
                let _ = actions.add_close(fd);
            }
            3 => {
                let path: &[u8] = match rng.next() % 4 {
                    0 => b"a",
                    1 => b"../b",
                    2 => b"/abs",
                    _ => b".",
                };
                let _ = actions.add_chdir(path);
            }
            4 => {
                let fd = (rng.next() % 12) as i32;
                let _ = actions.add_fchdir(fd);
            }
            5 => {
                let from = (rng.next() % 12) as i32;
                let _ = actions.add_closefrom(from);
            }
            _ => {
                let fd = (rng.next() % 12) as i32;
                let _ = actions.add_tcsetpgrp(fd);
            }
        }
    }
    actions
}

#[test]
fn two_thousand_seeded_action_lists_match_the_reference() {
    for seed in 1..=2000u64 {
        let mut rng = Rng(seed);
        let mut table = Vec::new();
        for fd in 0..3i32 {
            let present = rng.next() & 1 == 0;
            if present {
                table.push(FdEntry {
                    fd,
                    handle: 10 + fd as u32,
                    cloexec: rng.next() & 1 == 0,
                });
            }
        }
        let cwd = if rng.next() & 1 == 0 {
            b"/base".as_slice()
        } else {
            b"/".as_slice()
        };
        let cloexec_default = rng.next() & 1 == 0;
        let actions = build_actions(&mut rng);

        let mut real_env = ScriptedEnv::new();
        let real = apply_actions(&table, cwd, cloexec_default, &actions, &mut real_env);
        let mut ref_env = ScriptedEnv::new();
        let reference = reference(&table, cwd, cloexec_default, &actions, &mut ref_env);

        match (real, reference) {
            (Ok(real), Ok(reference)) => {
                assert_eq!(
                    real.fdmap, reference.fdmap,
                    "fdmap seed={seed} actions={actions:?}"
                );
                assert_eq!(real.cwd, reference.cwd, "cwd seed={seed}");
                assert_eq!(
                    real.temp_handles, reference.temp_handles,
                    "temp seed={seed}"
                );
                assert_eq!(real.tcsetpgrp_fd, reference.tcsetpgrp_fd, "tc seed={seed}");
            }
            (Err(real), Err(reference)) => {
                assert_eq!(real, reference, "errno seed={seed}");
                assert_eq!(real_env.closed, ref_env.closed, "closed temps seed={seed}");
            }
            (real, reference) => {
                panic!("outcome differs seed={seed}: {real:?} vs {reference:?}");
            }
        }
    }
}
