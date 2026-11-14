// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawn(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EINVAL, ENOENT};
use raven_libc_abi::structs::{PosixSpawnFileActionsT, PosixSpawnattrT};
use raven_libc_proc::spawn_model::SpawnCtx;

/// Spawn `path` with the caller's attributes, file actions and arrays.
///
/// # Safety
/// Every pointer follows its C contract; the two string arrays are NULL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawn(
    pid: *mut c_int,
    path: *const c_char,
    actions: *const PosixSpawnFileActionsT,
    attr: *const PosixSpawnattrT,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    if pid.is_null() {
        return EINVAL;
    }
    if path.is_null() {
        return ENOENT;
    }
    // SAFETY: the caller's two string arrays and path.
    let (program, arguments, environment) = unsafe {
        (
            crate::ffi::cstr::slice(path),
            crate::ffi::argv::collect(argv),
            crate::ffi::argv::collect(envp),
        )
    };
    let (arguments, environment) = match (arguments, environment) {
        (Ok(arguments), Ok(environment)) => (arguments, environment),
        (Err(error), _) | (_, Err(error)) => return i32::from(error.0),
    };
    let action_view = if actions.is_null() {
        None
    } else {
        // SAFETY: the caller's initialised file-action blob.
        Some(unsafe { crate::ffi::map_proc::actions_view(actions.cast_mut()) })
    };
    let action_list = action_view.as_ref().and_then(|view| view.get());
    let model = if attr.is_null() {
        None
    } else {
        // SAFETY: the caller's initialised attribute blob.
        Some(crate::ffi::map_proc::attr_model(unsafe {
            crate::ffi::map_proc::attr_view(attr.cast_mut())
        }))
    };
    let cwd = crate::ffi::platform::global().cwd();
    let ctx = SpawnCtx {
        table: &[],
        cwd: &cwd,
        sigmask: [0; 128],
        ignored: [0; 128],
        umask: crate::ffi::platform::global().umask(),
    };
    let mut env = crate::process::spawn_env::ShellSpawnEnv;
    match crate::ffi::map_proc::spawn(
        &crate::ffi::procbackend::backend(),
        &mut env,
        &ctx,
        program,
        false,
        None,
        action_list,
        model.as_ref(),
        &arguments,
        &environment,
    ) {
        Ok(child) => {
            // SAFETY: the caller's non-NULL pid slot.
            unsafe { pid.write(child) };
            0
        }
        Err(error) => error,
    }
}
