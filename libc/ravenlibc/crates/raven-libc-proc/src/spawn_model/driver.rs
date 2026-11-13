// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `posix_spawn` driver.

use alloc::vec::Vec;

use crate::backend::ProcBackend;
use crate::consts::{
    POSIX_SPAWN_CLOEXEC_DEFAULT, POSIX_SPAWN_SETSIGDEF, POSIX_SPAWN_SETSIGMASK, S_CLOEXEC_DEFAULT,
    S_USE_PATH_HINT,
};
use crate::errors::{E2BIG, ENOENT, ENOSYS, Errno};
use crate::exec_path::{script_argv, search_path};
use crate::limits::{ARG_MAX, MAX_ARGS};
use crate::spawn_model::api::{FileActions, SpawnAttr};
use crate::spawn_model::apply::{FdEntry, Scratch, SpawnEnv, apply_actions};
use crate::spawnp::spawnp_prechecks;
use crate::wire::SpawnRequest;

/// The parent-side context of a spawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnCtx<'a> {
    /// Inherited fd table.
    pub table: &'a [FdEntry],
    /// Current working directory.
    pub cwd: &'a [u8],
    /// Parent signal mask.
    pub sigmask: [u8; 128],
    /// Parent ignored-signal set.
    pub ignored: [u8; 128],
    /// Parent umask.
    pub umask: u32,
}

/// Build the `SPAWN` request and return the child pid.
#[allow(clippy::too_many_arguments)] // mirrors the C entry point
pub fn spawn_model_pid(
    backend: &dyn ProcBackend,
    env: &mut dyn SpawnEnv,
    ctx: &SpawnCtx<'_>,
    path: &[u8],
    use_path: bool,
    path_env: Option<&[u8]>,
    actions: Option<&FileActions>,
    attr: Option<&SpawnAttr>,
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
) -> Result<i32, Errno> {
    validate(path, argv, envp)?;
    let owned_attr;
    let attr = match attr {
        Some(value) => value,
        None => {
            owned_attr = SpawnAttr::new();
            &owned_attr
        }
    };
    let owned_actions;
    let actions = match actions {
        Some(value) => value,
        None => {
            owned_actions = FileActions::new();
            &owned_actions
        }
    };
    let cloexec_default = attr.flags & POSIX_SPAWN_CLOEXEC_DEFAULT != 0;
    let scratch = apply_actions(ctx.table, ctx.cwd, cloexec_default, actions, env)?;
    let result = (|| {
        if use_path && spawnp_prechecks(path)? {
            let mut diag = |_message: &[u8]| {};
            search_path(
                path,
                path_env,
                b"posix_spawnp",
                &mut |attempt| {
                    let (spawn_path, spawn_argv) = if attempt.shell_fallback {
                        (b"/bin/sh".as_slice(), script_argv(argv, attempt.path))
                    } else {
                        (attempt.path, argv.to_vec())
                    };
                    let request =
                        build_request(ctx, attr, &scratch, use_path, spawn_path, &spawn_argv, envp);
                    backend.spawn(&request)
                },
                &mut diag,
            )
        } else {
            let request = build_request(ctx, attr, &scratch, use_path, path, argv, envp);
            backend.spawn(&request)
        }
    })();
    for handle in &scratch.temp_handles {
        env.close_temp(*handle);
    }
    result
}

/// The C-facing `posix_spawn` model: zero or the errno value, never -1.
#[allow(clippy::too_many_arguments)] // mirrors the C entry point
pub fn posix_spawn_model(
    backend: &dyn ProcBackend,
    env: &mut dyn SpawnEnv,
    ctx: &SpawnCtx<'_>,
    path: &[u8],
    use_path: bool,
    path_env: Option<&[u8]>,
    actions: Option<&FileActions>,
    attr: Option<&SpawnAttr>,
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
) -> i32 {
    match spawn_model_pid(
        backend, env, ctx, path, use_path, path_env, actions, attr, argv, envp,
    ) {
        Ok(_) => 0,
        Err(error) => error,
    }
}

/// Spawn with only a signal mask and default set, for `system()`.
pub fn spawn_with_mask(
    backend: &dyn ProcBackend,
    ctx: &SpawnCtx<'_>,
    path: &[u8],
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
    mask: [u8; 128],
    sigdefault: [u8; 128],
) -> Result<i32, Errno> {
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_SETSIGMASK | POSIX_SPAWN_SETSIGDEF)?;
    attr.set_sigmask(mask);
    attr.set_sigdefault(sigdefault);
    let mut env = NoopEnv;
    spawn_model_pid(
        backend,
        &mut env,
        ctx,
        path,
        false,
        None,
        None,
        Some(&attr),
        argv,
        envp,
    )
}

fn build_request(
    ctx: &SpawnCtx<'_>,
    attr: &SpawnAttr,
    scratch: &Scratch,
    use_path: bool,
    path: &[u8],
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
) -> SpawnRequest {
    let mut spawn_flags = u32::from((attr.flags & crate::consts::POSIX_SPAWN_ALL) as u16);
    if use_path {
        spawn_flags |= S_USE_PATH_HINT;
    }
    if attr.flags & POSIX_SPAWN_CLOEXEC_DEFAULT != 0 {
        spawn_flags |= S_CLOEXEC_DEFAULT;
    }
    let sigmask = if attr.flags & POSIX_SPAWN_SETSIGMASK != 0 {
        attr.sigmask
    } else {
        ctx.sigmask
    };
    SpawnRequest {
        spawn_flags,
        pgroup: attr.pgroup,
        sigdefault: attr.sigdefault,
        sigmask,
        ignored: ctx.ignored,
        sched_policy: attr.sched_policy,
        sched_prio: attr.sched_priority,
        umask: ctx.umask,
        tcpgrp_fd: scratch.tcsetpgrp_fd,
        path: path.to_vec(),
        cwd: scratch.cwd.clone(),
        argv: argv.to_vec(),
        envp: envp.to_vec(),
        fdmap: scratch.fdmap.clone(),
    }
}

fn validate(path: &[u8], argv: &[Vec<u8>], envp: &[Vec<u8>]) -> Result<(), Errno> {
    if path.is_empty() {
        return Err(ENOENT);
    }
    if argv.len() > MAX_ARGS {
        return Err(E2BIG);
    }
    let mut bytes = 0usize;
    for argument in argv {
        bytes = bytes
            .checked_add(argument.len())
            .and_then(|total| total.checked_add(1))
            .ok_or(E2BIG)?;
        if bytes > ARG_MAX {
            return Err(E2BIG);
        }
    }
    for variable in envp {
        bytes = bytes
            .checked_add(variable.len())
            .and_then(|total| total.checked_add(1))
            .ok_or(E2BIG)?;
        if bytes > ARG_MAX {
            return Err(E2BIG);
        }
    }
    Ok(())
}

struct NoopEnv;

impl SpawnEnv for NoopEnv {
    fn open(&mut self, _path: &[u8], _oflag: i32, _mode: u32) -> Result<u32, Errno> {
        Err(ENOSYS)
    }

    fn is_dir(&mut self, _handle: u32) -> Result<bool, Errno> {
        Err(ENOSYS)
    }

    fn close_temp(&mut self, _handle: u32) {}

    fn fd_path(&mut self, _handle: u32) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
#[path = "driver_ut.rs"]
mod tests;
