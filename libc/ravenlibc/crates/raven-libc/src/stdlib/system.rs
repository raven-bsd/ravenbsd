// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `system(3)`: the C41/C53 spawn-first protocol (A-stdlib-11) over the shell's process backend.
//!
//! `None` returns 1 without touching anything. The signal steps of the protocol are inert
//! because the C58 signal crate is not on this branch (there is no handler to block, ignore or
//! restore); the spawn and wait halves are real. Until `procd` exists the backend reports
//! `ENOSYS`, which the function returns as `-1` with `errno = ENOSYS` (RD-20).

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::Errno;
use raven_libc_proc::backend::ProcBackend;
use raven_libc_proc::consts::P_PID;
use raven_libc_proc::spawn_model::SpawnCtx;
use raven_libc_stdlib::proc::{Sig, SystemBackend};

struct ShellSystem<'a, B: ProcBackend> {
    backend: &'a B,
    error: Option<Errno>,
}

impl<B: ProcBackend> SystemBackend for ShellSystem<'_, B> {
    fn block(&mut self, _signals: &[Sig]) {
        // C58's disposition table and signal mask are not on this branch; the spawn path below
        // is the half this ticket owns.
    }

    fn spawn_sh(&mut self, command: &[u8]) -> Result<i32, Errno> {
        let cwd = crate::ffi::platform::global().cwd();
        let ctx = SpawnCtx {
            table: &[],
            cwd: &cwd,
            sigmask: [0; 128],
            ignored: [0; 128],
            umask: crate::ffi::platform::global().umask(),
        };
        let argv = alloc::vec![b"sh".to_vec(), b"-c".to_vec(), command.to_vec()];
        let envp = crate::process::exec_common::current_envp()?;
        let mut env = crate::process::spawn_env::ShellSpawnEnv;
        let result = crate::ffi::map_proc::spawn(
            self.backend,
            &mut env,
            &ctx,
            b"/bin/sh",
            false,
            None,
            None,
            None,
            &argv,
            &envp,
        )
        .map_err(crate::ffi::map_proc::error);
        if let Err(error) = result {
            self.error = Some(error);
        }
        result
    }

    fn ignore(&mut self, _signals: &[Sig]) {}

    fn unblock_and_restore(&mut self) {}

    fn wait(&mut self, pid: i32) -> Result<i32, Errno> {
        self.backend
            .wait(P_PID, i64::from(pid), 0)
            .map(|result| result.status)
            .map_err(crate::ffi::map_proc::error)
    }
}

/// Run `command` through `/bin/sh -c`; NULL returns 1 (a shell is available).
///
/// # Safety
/// `command` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn system(command: *const c_char) -> c_int {
    if command.is_null() {
        return 1;
    }
    // SAFETY: the caller's NUL-terminated command line.
    let command = unsafe { crate::ffi::cstr::slice(command) };
    let backend = crate::ffi::procbackend::backend();
    let mut shell = ShellSystem {
        backend: &backend,
        error: None,
    };
    let status = raven_libc_stdlib::proc::system(&mut shell, Some(command));
    if status == -1 {
        let error = shell
            .error
            .unwrap_or(Errno(raven_libc_abi::errno::ENOSYS as u16));
        crate::ffi::errno::set(error);
    }
    status
}
