// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The process family of the C-ABI shell (C64 4.6): `fork`, the `exec` forms, `posix_spawn`,
//! the wait family, identity, resource limits, `daemon`, `system` and the program-name data.
//!
//! Every export is a pointer check plus one call into `raven-libc-proc` through
//! [`crate::ffi::map_proc`]; the backend is [`crate::ffi::procbackend`]. `fork`, `execve` and
//! `posix_spawn` report the backend's `ENOSYS` until `procd` exists (H-18, PG-44/45/46); the
//! wait and identity calls keep the crate's local answers.

#[allow(non_snake_case)]
pub(crate) mod _Fork;
pub(crate) mod __progname;
pub(crate) mod __progname_full;
pub(crate) mod __raven_va_execl;
pub(crate) mod __raven_va_execle;
pub(crate) mod __raven_va_execlp;
pub(crate) mod daemon;
pub(crate) mod daemon_ops;
pub(crate) mod exec_common;
pub(crate) mod execl;
pub(crate) mod execle;
pub(crate) mod execlp;
pub(crate) mod execv;
pub(crate) mod execve;
pub(crate) mod execvp;
pub(crate) mod execvpe;
pub(crate) mod fexecve;
pub(crate) mod fork;
pub(crate) mod getegid;
pub(crate) mod geteuid;
pub(crate) mod getgid;
pub(crate) mod getgroups;
pub(crate) mod getlogin;
pub(crate) mod getlogin_r;
pub(crate) mod getpgid;
pub(crate) mod getpgrp;
pub(crate) mod getpid;
pub(crate) mod getppid;
pub(crate) mod getpriority;
pub(crate) mod getprogname;
pub(crate) mod getrlimit;
pub(crate) mod getrusage;
pub(crate) mod getsid;
pub(crate) mod getuid;
pub(crate) mod initgroups;
pub(crate) mod issetugid;
pub(crate) mod nice;
pub(crate) mod posix_spawn;
pub(crate) mod posix_spawn_file_actions_addchdir_np;
pub(crate) mod posix_spawn_file_actions_addclose;
pub(crate) mod posix_spawn_file_actions_addclosefrom_np;
pub(crate) mod posix_spawn_file_actions_adddup2;
pub(crate) mod posix_spawn_file_actions_addfchdir_np;
pub(crate) mod posix_spawn_file_actions_addopen;
pub(crate) mod posix_spawn_file_actions_destroy;
pub(crate) mod posix_spawn_file_actions_init;
pub(crate) mod posix_spawnattr_destroy;
pub(crate) mod posix_spawnattr_getflags;
pub(crate) mod posix_spawnattr_getpgroup;
pub(crate) mod posix_spawnattr_getschedparam;
pub(crate) mod posix_spawnattr_getschedpolicy;
pub(crate) mod posix_spawnattr_getsigdefault;
pub(crate) mod posix_spawnattr_getsigmask;
pub(crate) mod posix_spawnattr_init;
pub(crate) mod posix_spawnattr_setflags;
pub(crate) mod posix_spawnattr_setpgroup;
pub(crate) mod posix_spawnattr_setschedparam;
pub(crate) mod posix_spawnattr_setschedpolicy;
pub(crate) mod posix_spawnattr_setsigdefault;
pub(crate) mod posix_spawnattr_setsigmask;
pub(crate) mod posix_spawnp;
pub(crate) mod prlimit;
pub(crate) mod program_invocation_name;
pub(crate) mod program_invocation_short_name;
pub(crate) mod setegid;
pub(crate) mod seteuid;
pub(crate) mod setgid;
pub(crate) mod setgroups;
pub(crate) mod setpgid;
pub(crate) mod setpriority;
pub(crate) mod setprogname;
pub(crate) mod setregid;
pub(crate) mod setreuid;
pub(crate) mod setrlimit;
pub(crate) mod setsid;
pub(crate) mod setuid;
pub(crate) mod spawn_env;
pub(crate) mod times;
pub(crate) mod va_args;
pub(crate) mod vfork;
pub(crate) mod wait;
pub(crate) mod wait3;
pub(crate) mod wait4;
pub(crate) mod waitid;
pub(crate) mod waitpid;
