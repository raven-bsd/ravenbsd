// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `exec` common path: argument validation, the current environment, the `PATH` search of
//! C53's `exec_path` and the one request. `execve`/`execv`/`execvp`/`execvpe` differ only in
//! the three inputs this module takes.

use core::ffi::c_int;

use raven_libc_abi::errno::{EFAULT, Errno};

/// The current `environ` as owned argument strings.
pub(crate) fn current_envp() -> Result<alloc::vec::Vec<alloc::vec::Vec<u8>>, Errno> {
    // SAFETY: `environ` is C61's NULL-terminated environment array.
    let pointer = unsafe { core::ptr::read(&raw const crate::stdlib::environ::environ) };
    // SAFETY: the array contract is the C `char *const[]` contract.
    unsafe { crate::ffi::argv::collect(pointer.cast()) }
}

/// The `PATH` value of the current environment.
pub(crate) fn path_env() -> Option<&'static [u8]> {
    crate::stdlib::env_support::get(b"PATH")
}

/// Execute `path` with the given arrays; `search` walks `PATH` first.
pub(crate) fn run(
    path: &[u8],
    argv: alloc::vec::Vec<alloc::vec::Vec<u8>>,
    envp: alloc::vec::Vec<alloc::vec::Vec<u8>>,
    fd: Option<i32>,
    search: bool,
) -> c_int {
    let backend = crate::ffi::procbackend::backend();
    let cwd = crate::ffi::platform::global().cwd();
    if !search {
        let value = crate::ffi::map_proc::exec(&backend, &cwd, path, &argv, &envp, fd);
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::error(value)));
    }
    let mut diag = |_message: &[u8]| {};
    let result = raven_libc_proc::exec_path::search_path(
        path,
        path_env(),
        b"execvp",
        &mut |attempt| {
            let (candidate, final_argv) = if attempt.shell_fallback {
                (
                    b"/bin/sh".as_slice(),
                    raven_libc_proc::exec_path::script_argv(&argv, attempt.path),
                )
            } else {
                (attempt.path, argv.clone())
            };
            let value =
                crate::ffi::map_proc::exec(&backend, &cwd, candidate, &final_argv, &envp, fd);
            Err(value)
        },
        &mut diag,
    );
    match result {
        Ok(()) => 0,
        Err(value) => crate::ffi::ret::int(Err(crate::ffi::map_proc::error(value))),
    }
}

/// The `EFAULT` value as the shell's error type.
pub(crate) fn fault() -> Errno {
    Errno(EFAULT as u16)
}
