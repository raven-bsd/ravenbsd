// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Numeric constants of the process interfaces.
//!
//! Flag values are the Linux/musl values used by the headers and the procd wire protocol; the
//! FreeBSD `POSIX_SPAWN_*` and BSD `wait` values are deliberately not used.

/// `POSIX_SPAWN_RESETIDS`.
pub const POSIX_SPAWN_RESETIDS: i16 = 1;
/// `POSIX_SPAWN_SETPGROUP`.
pub const POSIX_SPAWN_SETPGROUP: i16 = 2;
/// `POSIX_SPAWN_SETSIGDEF`.
pub const POSIX_SPAWN_SETSIGDEF: i16 = 4;
/// `POSIX_SPAWN_SETSIGMASK`.
pub const POSIX_SPAWN_SETSIGMASK: i16 = 8;
/// `POSIX_SPAWN_SETSCHEDPARAM`.
pub const POSIX_SPAWN_SETSCHEDPARAM: i16 = 16;
/// `POSIX_SPAWN_SETSCHEDULER`.
pub const POSIX_SPAWN_SETSCHEDULER: i16 = 32;
/// `POSIX_SPAWN_USEVFORK` (accepted and ignored).
pub const POSIX_SPAWN_USEVFORK: i16 = 64;
/// `POSIX_SPAWN_SETSID`.
pub const POSIX_SPAWN_SETSID: i16 = 128;
/// `POSIX_SPAWN_CLOEXEC_DEFAULT` (Raven extension, musl-compatible value).
pub const POSIX_SPAWN_CLOEXEC_DEFAULT: i16 = 0x2000;
/// Every flag bit accepted by [`crate::spawn_model::SpawnAttr::set_flags`].
pub const POSIX_SPAWN_ALL: i16 = POSIX_SPAWN_RESETIDS
    | POSIX_SPAWN_SETPGROUP
    | POSIX_SPAWN_SETSIGDEF
    | POSIX_SPAWN_SETSIGMASK
    | POSIX_SPAWN_SETSCHEDPARAM
    | POSIX_SPAWN_SETSCHEDULER
    | POSIX_SPAWN_USEVFORK
    | POSIX_SPAWN_SETSID
    | POSIX_SPAWN_CLOEXEC_DEFAULT;

/// Spawn hint: the path is a `PATH` search result.
pub const S_USE_PATH_HINT: u32 = 1 << 16;
/// Spawn hint: all inherited descriptors start close-on-exec.
pub const S_CLOEXEC_DEFAULT: u32 = 1 << 17;

/// Wait for a state change without blocking.
pub const WNOHANG: u32 = 1;
/// Report stopped children.
pub const WUNTRACED: u32 = 2;
/// `WSTOPPED` alias.
pub const WSTOPPED: u32 = 2;
/// Report exited children.
pub const WEXITED: u32 = 4;
/// Report continued children.
pub const WCONTINUED: u32 = 8;
/// Leave the child waitable.
pub const WNOWAIT: u32 = 0x0100_0000;
/// BSD compatibility bit, accepted and ignored.
pub const __WALL: u32 = 0x4000_0000;
/// Every option bit accepted by `wait*`.
pub const WAIT_OPTION_MASK: u32 = WNOHANG | WUNTRACED | WEXITED | WCONTINUED | WNOWAIT | __WALL;

/// Wait for any child.
pub const P_ALL: u32 = 0;
/// Wait for one pid.
pub const P_PID: u32 = 1;
/// Wait for one process group.
pub const P_PGID: u32 = 2;
/// Wait for one pidfd.
pub const P_PIDFD: u32 = 3;

/// `RLIMIT_CPU`.
pub const RLIMIT_CPU: u32 = 0;
/// `RLIMIT_FSIZE`.
pub const RLIMIT_FSIZE: u32 = 1;
/// `RLIMIT_DATA`.
pub const RLIMIT_DATA: u32 = 2;
/// `RLIMIT_STACK`.
pub const RLIMIT_STACK: u32 = 3;
/// `RLIMIT_CORE`.
pub const RLIMIT_CORE: u32 = 4;
/// `RLIMIT_RSS`.
pub const RLIMIT_RSS: u32 = 5;
/// `RLIMIT_NPROC`.
pub const RLIMIT_NPROC: u32 = 6;
/// `RLIMIT_NOFILE`.
pub const RLIMIT_NOFILE: u32 = 7;
/// `RLIMIT_MEMLOCK`.
pub const RLIMIT_MEMLOCK: u32 = 8;
/// `RLIMIT_AS`.
pub const RLIMIT_AS: u32 = 9;
/// `RLIMIT_LOCKS`.
pub const RLIMIT_LOCKS: u32 = 10;
/// `RLIMIT_SIGPENDING`.
pub const RLIMIT_SIGPENDING: u32 = 11;
/// `RLIMIT_MSGQUEUE`.
pub const RLIMIT_MSGQUEUE: u32 = 12;
/// `RLIMIT_NICE`.
pub const RLIMIT_NICE: u32 = 13;
/// `RLIMIT_RTPRIO`.
pub const RLIMIT_RTPRIO: u32 = 14;
/// `RLIMIT_RTTIME`.
pub const RLIMIT_RTTIME: u32 = 15;
/// Number of resource limits.
pub const RLIMIT_NLIMITS: u32 = 16;
/// The unlimited resource value.
pub const RLIM_INFINITY: u64 = u64::MAX;

/// `PRIO_PROCESS`.
pub const PRIO_PROCESS: u32 = 0;
/// `PRIO_PGRP`.
pub const PRIO_PGRP: u32 = 1;
/// `PRIO_USER`.
pub const PRIO_USER: u32 = 2;

/// The normal scheduler policy.
pub const SCHED_OTHER: i32 = 0;
/// First-in, first-out scheduling.
pub const SCHED_FIFO: i32 = 1;
/// Round-robin scheduling.
pub const SCHED_RR: i32 = 2;
/// Batch scheduling (accepted, treated as `OTHER`).
pub const SCHED_BATCH: i32 = 3;
/// Idle scheduling (accepted, treated as `OTHER`).
pub const SCHED_IDLE: i32 = 5;
/// Reset policy on fork.
pub const SCHED_RESET_ON_FORK: i32 = 0x4000_0000;

/// `AT_NULL`.
pub const AT_NULL: usize = 0;
/// `AT_PHDR`.
pub const AT_PHDR: usize = 3;
/// `AT_PHENT`.
pub const AT_PHENT: usize = 4;
/// `AT_PHNUM`.
pub const AT_PHNUM: usize = 5;
/// `AT_PAGESZ`.
pub const AT_PAGESZ: usize = 6;
/// `AT_BASE`.
pub const AT_BASE: usize = 7;
/// `AT_FLAGS`.
pub const AT_FLAGS: usize = 8;
/// `AT_ENTRY`.
pub const AT_ENTRY: usize = 9;
/// `AT_UID`.
pub const AT_UID: usize = 11;
/// `AT_EUID`.
pub const AT_EUID: usize = 12;
/// `AT_GID`.
pub const AT_GID: usize = 13;
/// `AT_EGID`.
pub const AT_EGID: usize = 14;
/// `AT_PLATFORM`.
pub const AT_PLATFORM: usize = 15;
/// `AT_HWCAP`.
pub const AT_HWCAP: usize = 16;
/// `AT_CLKTCK`.
pub const AT_CLKTCK: usize = 17;
/// `AT_SECURE`.
pub const AT_SECURE: usize = 23;
/// `AT_RANDOM`.
pub const AT_RANDOM: usize = 25;
/// `AT_HWCAP2`.
pub const AT_HWCAP2: usize = 26;
/// `AT_EXECFN`.
pub const AT_EXECFN: usize = 31;
/// `AT_MINSIGSTKSZ`.
pub const AT_MINSIGSTKSZ: usize = 51;

/// Raven procd connection handle.
pub const AT_RAVEN_PROCD: usize = 0x7001;
/// Raven procinfo page address.
pub const AT_RAVEN_PROCINFO: usize = 0x7002;
/// Raven fdmap pointer.
pub const AT_RAVEN_FDMAP: usize = 0x7003;
/// Raven fdmap entry count.
pub const AT_RAVEN_FDMAP_N: usize = 0x7004;
/// Raven current directory pointer.
pub const AT_RAVEN_CWD: usize = 0x7005;
/// Raven ABI version.
pub const AT_RAVEN_ABI: usize = 0x7006;
/// Raven time page pointer.
pub const AT_RAVEN_TIMEPAGE: usize = 0x7007;

/// Default `PATH` used when the environment has no `PATH`.
pub const DEFPATH: &[u8] = b"/bin:/usr/bin";
