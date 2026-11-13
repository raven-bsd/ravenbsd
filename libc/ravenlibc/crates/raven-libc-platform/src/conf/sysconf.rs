// SPDX-License-Identifier: BSD-2-Clause
/*
 * Copyright (C) 2008 The Android Open Source Project
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *  * Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 *  * Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in
 *    the documentation and/or other materials provided with the
 *    distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
 * "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
 * LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
 * FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
 * COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
 * INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
 * BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS
 * OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
 * AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
 * OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from bionic:libc/bionic/sysconf.cpp @
// 731631f300090436d7f5df80d50b6275c8c60a93; the `_SC_*` switch and the POSIX
// minimums are kept; `sysconf_caches` and the three `__sysconf_caches` variants
// are deleted (cache sizes return 0), `get_nprocs`/`get_phys_pages`/
// `get_avphys_pages` are replaced by the `sysinfo` hook with the fixed fallback,
// `__sysconf_rlimit` by the libc rlimit cell, and the argument list is the
// Raven value set of doc 11 section 3.2.

//! `sysconf` on Raven.
//!
//! The switch follows bionic. Cache geometry is zero: Raven has no `AT_L1*` auxv keys and a
//! fabricated value would be worse than zero. CPU and memory counts come from the `sysinfo`
//! hook (PG-21); the fallback is one CPU and the documented 64 MiB memory estimate.

use raven_abi::errno::Errno;
use raven_libc_abi::{limits, sysconf};

/// Inputs the caller (the shell) supplies from the rlimit and `sysinfo` cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SysconfInputs {
    /// Online CPUs.
    pub cpus_online: i64,
    /// Configured CPUs.
    pub cpus_conf: i64,
    /// Total physical pages.
    pub phys_pages: i64,
    /// Available physical pages.
    pub avphys_pages: i64,
    /// Soft `RLIMIT_NOFILE`.
    pub open_max: i64,
    /// Soft `RLIMIT_NPROC`.
    pub child_max: i64,
    /// `RLIMIT_STACK` used for `_SC_ARG_MAX`.
    pub stack_max: i64,
}

impl Default for SysconfInputs {
    fn default() -> Self {
        Self {
            cpus_online: 1,
            cpus_conf: 1,
            phys_pages: (64 * 1024 * 1024) / 4096,
            avphys_pages: (32 * 1024 * 1024) / 4096,
            open_max: i64::from(crate::fd::DEFAULT_SOFT_NOFILE),
            child_max: 1024,
            stack_max: 8 * 1024 * 1024,
        }
    }
}

// POSIX minimums and Raven values from doc 11 section 3.2.
const POSIX_VERSION: i64 = 200_809;
const POSIX2_VERSION: i64 = 200_809;
const POSIX2_C_BIND: i64 = 1;
const POSIX2_C_DEV: i64 = 1;
const POSIX2_CHAR_TERM: i64 = 1;
const POSIX2_LOCALEDEF: i64 = 1;
const POSIX2_SW_DEV: i64 = 1;
const POSIX2_UPE: i64 = 1;
const POSIX_JOB_CONTROL: i64 = 1;
const POSIX_SAVED_IDS: i64 = 1;
const POSIX_RE_DUP_MAX: i64 = 255;
const POSIX_TZNAME_MAX: i64 = 6;
const POSIX2_BC_BASE_MAX: i64 = 99;
const POSIX2_BC_DIM_MAX: i64 = 2048;
const POSIX2_BC_SCALE_MAX: i64 = 99;
const POSIX2_BC_STRING_MAX: i64 = 2047;
const POSIX2_COLL_WEIGHTS_MAX: i64 = 2;
const POSIX2_EXPR_NEST_MAX: i64 = 32;
const POSIX2_LINE_MAX: i64 = 2048;
const XOPEN_VERSION: i64 = 700;
const FOPEN_MAX: i64 = 16;

// Linux numbering for the cache-geometry names; the ABI crate does not export them yet.
const SC_LEVEL1_ICACHE_SIZE: i32 = 185;
const SC_LEVEL1_ICACHE_ASSOC: i32 = 186;
const SC_LEVEL1_ICACHE_LINESIZE: i32 = 187;
const SC_LEVEL1_DCACHE_SIZE: i32 = 188;
const SC_LEVEL1_DCACHE_ASSOC: i32 = 189;
const SC_LEVEL1_DCACHE_LINESIZE: i32 = 190;
const SC_LEVEL2_CACHE_SIZE: i32 = 191;
const SC_LEVEL2_CACHE_ASSOC: i32 = 192;
const SC_LEVEL2_CACHE_LINESIZE: i32 = 193;
const SC_LEVEL3_CACHE_SIZE: i32 = 194;
const SC_LEVEL3_CACHE_ASSOC: i32 = 195;
const SC_LEVEL3_CACHE_LINESIZE: i32 = 196;
const SC_LEVEL4_CACHE_SIZE: i32 = 197;
const SC_LEVEL4_CACHE_ASSOC: i32 = 198;
const SC_LEVEL4_CACHE_LINESIZE: i32 = 199;

/// Every `_SC_*` id this implementation names, as `(name, value)`.
///
/// The fixture test walks this list and requires every id defined by the ABI to appear either
/// here or in the documented `EINVAL` set below.
pub const NAMED: &[(&str, i32)] = &[
    ("_SC_ARG_MAX", sysconf::_SC_ARG_MAX),
    ("_SC_CHILD_MAX", sysconf::_SC_CHILD_MAX),
    ("_SC_CLK_TCK", sysconf::_SC_CLK_TCK),
    ("_SC_NGROUPS_MAX", sysconf::_SC_NGROUPS_MAX),
    ("_SC_OPEN_MAX", sysconf::_SC_OPEN_MAX),
    ("_SC_STREAM_MAX", sysconf::_SC_STREAM_MAX),
    ("_SC_TZNAME_MAX", sysconf::_SC_TZNAME_MAX),
    ("_SC_JOB_CONTROL", sysconf::_SC_JOB_CONTROL),
    ("_SC_SAVED_IDS", sysconf::_SC_SAVED_IDS),
    ("_SC_VERSION", sysconf::_SC_VERSION),
    ("_SC_PAGESIZE", sysconf::_SC_PAGESIZE),
    ("_SC_PAGE_SIZE", sysconf::_SC_PAGE_SIZE),
    ("_SC_NPROCESSORS_CONF", sysconf::_SC_NPROCESSORS_CONF),
    ("_SC_NPROCESSORS_ONLN", sysconf::_SC_NPROCESSORS_ONLN),
    ("_SC_PHYS_PAGES", sysconf::_SC_PHYS_PAGES),
    ("_SC_AVPHYS_PAGES", sysconf::_SC_AVPHYS_PAGES),
    ("_SC_IOV_MAX", sysconf::_SC_IOV_MAX),
    ("_SC_HOST_NAME_MAX", sysconf::_SC_HOST_NAME_MAX),
    ("_SC_LOGIN_NAME_MAX", sysconf::_SC_LOGIN_NAME_MAX),
    ("_SC_TTY_NAME_MAX", sysconf::_SC_TTY_NAME_MAX),
    ("_SC_SYMLOOP_MAX", sysconf::_SC_SYMLOOP_MAX),
    ("_SC_NZERO", sysconf::_SC_NZERO),
    ("_SC_ATEXIT_MAX", sysconf::_SC_ATEXIT_MAX),
    ("_SC_PASS_MAX", sysconf::_SC_PASS_MAX),
    ("_SC_LEVEL1_ICACHE_SIZE", SC_LEVEL1_ICACHE_SIZE),
    ("_SC_LEVEL2_CACHE_SIZE", SC_LEVEL2_CACHE_SIZE),
    ("_SC_LEVEL3_CACHE_SIZE", SC_LEVEL3_CACHE_SIZE),
    ("_SC_LEVEL4_CACHE_SIZE", SC_LEVEL4_CACHE_SIZE),
    ("_SC_2_VERSION", sysconf::_SC_2_VERSION),
    ("_SC_2_C_BIND", sysconf::_SC_2_C_BIND),
    ("_SC_2_C_DEV", sysconf::_SC_2_C_DEV),
    ("_SC_2_CHAR_TERM", sysconf::_SC_2_CHAR_TERM),
    ("_SC_2_LOCALEDEF", sysconf::_SC_2_LOCALEDEF),
    ("_SC_2_SW_DEV", sysconf::_SC_2_SW_DEV),
    ("_SC_2_UPE", sysconf::_SC_2_UPE),
    ("_SC_XOPEN_VERSION", sysconf::_SC_XOPEN_VERSION),
    ("_SC_XOPEN_UNIX", sysconf::_SC_XOPEN_UNIX),
    ("_SC_XOPEN_CRYPT", sysconf::_SC_XOPEN_CRYPT),
    ("_SC_XOPEN_ENH_I18N", sysconf::_SC_XOPEN_ENH_I18N),
    ("_SC_XOPEN_SHM", sysconf::_SC_XOPEN_SHM),
    ("_SC_XOPEN_REALTIME", sysconf::_SC_XOPEN_REALTIME),
    (
        "_SC_XOPEN_REALTIME_THREADS",
        sysconf::_SC_XOPEN_REALTIME_THREADS,
    ),
    ("_SC_XOPEN_LEGACY", sysconf::_SC_XOPEN_LEGACY),
    ("_SC_BC_BASE_MAX", sysconf::_SC_BC_BASE_MAX),
    ("_SC_BC_DIM_MAX", sysconf::_SC_BC_DIM_MAX),
    ("_SC_BC_SCALE_MAX", sysconf::_SC_BC_SCALE_MAX),
    ("_SC_BC_STRING_MAX", sysconf::_SC_BC_STRING_MAX),
    ("_SC_COLL_WEIGHTS_MAX", sysconf::_SC_COLL_WEIGHTS_MAX),
    ("_SC_EXPR_NEST_MAX", sysconf::_SC_EXPR_NEST_MAX),
    ("_SC_LINE_MAX", sysconf::_SC_LINE_MAX),
    ("_SC_RE_DUP_MAX", sysconf::_SC_RE_DUP_MAX),
    ("_SC_GETGR_R_SIZE_MAX", sysconf::_SC_GETGR_R_SIZE_MAX),
    ("_SC_GETPW_R_SIZE_MAX", sysconf::_SC_GETPW_R_SIZE_MAX),
    (
        "_SC_THREAD_DESTRUCTOR_ITERATIONS",
        sysconf::_SC_THREAD_DESTRUCTOR_ITERATIONS,
    ),
    ("_SC_THREAD_KEYS_MAX", sysconf::_SC_THREAD_KEYS_MAX),
    ("_SC_THREAD_STACK_MIN", sysconf::_SC_THREAD_STACK_MIN),
    ("_SC_THREAD_THREADS_MAX", sysconf::_SC_THREAD_THREADS_MAX),
    ("_SC_THREADS", sysconf::_SC_THREADS),
    (
        "_SC_THREAD_SAFE_FUNCTIONS",
        sysconf::_SC_THREAD_SAFE_FUNCTIONS,
    ),
    (
        "_SC_THREAD_ATTR_STACKADDR",
        sysconf::_SC_THREAD_ATTR_STACKADDR,
    ),
    (
        "_SC_THREAD_ATTR_STACKSIZE",
        sysconf::_SC_THREAD_ATTR_STACKSIZE,
    ),
    ("_SC_MONOTONIC_CLOCK", sysconf::_SC_MONOTONIC_CLOCK),
    ("_SC_AIO_LISTIO_MAX", sysconf::_SC_AIO_LISTIO_MAX),
    ("_SC_AIO_MAX", sysconf::_SC_AIO_MAX),
    ("_SC_AIO_PRIO_DELTA_MAX", sysconf::_SC_AIO_PRIO_DELTA_MAX),
    ("_SC_DELAYTIMER_MAX", sysconf::_SC_DELAYTIMER_MAX),
    ("_SC_MQ_OPEN_MAX", sysconf::_SC_MQ_OPEN_MAX),
    ("_SC_MQ_PRIO_MAX", sysconf::_SC_MQ_PRIO_MAX),
    ("_SC_RTSIG_MAX", sysconf::_SC_RTSIG_MAX),
    ("_SC_SEM_NSEMS_MAX", sysconf::_SC_SEM_NSEMS_MAX),
    ("_SC_SEM_VALUE_MAX", sysconf::_SC_SEM_VALUE_MAX),
    ("_SC_SIGQUEUE_MAX", sysconf::_SC_SIGQUEUE_MAX),
    ("_SC_TIMER_MAX", sysconf::_SC_TIMER_MAX),
    ("_SC_ASYNCHRONOUS_IO", sysconf::_SC_ASYNCHRONOUS_IO),
    ("_SC_FSYNC", sysconf::_SC_FSYNC),
    ("_SC_MAPPED_FILES", sysconf::_SC_MAPPED_FILES),
    ("_SC_MEMLOCK", sysconf::_SC_MEMLOCK),
    ("_SC_MEMLOCK_RANGE", sysconf::_SC_MEMLOCK_RANGE),
    ("_SC_MEMORY_PROTECTION", sysconf::_SC_MEMORY_PROTECTION),
    ("_SC_MESSAGE_PASSING", sysconf::_SC_MESSAGE_PASSING),
    ("_SC_PRIORITIZED_IO", sysconf::_SC_PRIORITIZED_IO),
    ("_SC_PRIORITY_SCHEDULING", sysconf::_SC_PRIORITY_SCHEDULING),
    ("_SC_REALTIME_SIGNALS", sysconf::_SC_REALTIME_SIGNALS),
    ("_SC_SEMAPHORES", sysconf::_SC_SEMAPHORES),
    (
        "_SC_SHARED_MEMORY_OBJECTS",
        sysconf::_SC_SHARED_MEMORY_OBJECTS,
    ),
    ("_SC_SYNCHRONIZED_IO", sysconf::_SC_SYNCHRONIZED_IO),
    ("_SC_TIMERS", sysconf::_SC_TIMERS),
    ("_SC_ADVISORY_INFO", sysconf::_SC_ADVISORY_INFO),
    ("_SC_BARRIERS", sysconf::_SC_BARRIERS),
    ("_SC_CLOCK_SELECTION", sysconf::_SC_CLOCK_SELECTION),
    ("_SC_CPUTIME", sysconf::_SC_CPUTIME),
    ("_SC_IPV6", sysconf::_SC_IPV6),
    ("_SC_RAW_SOCKETS", sysconf::_SC_RAW_SOCKETS),
    ("_SC_READER_WRITER_LOCKS", sysconf::_SC_READER_WRITER_LOCKS),
    ("_SC_REGEXP", sysconf::_SC_REGEXP),
    ("_SC_SHELL", sysconf::_SC_SHELL),
    ("_SC_SPAWN", sysconf::_SC_SPAWN),
    ("_SC_SPIN_LOCKS", sysconf::_SC_SPIN_LOCKS),
    ("_SC_SPORADIC_SERVER", sysconf::_SC_SPORADIC_SERVER),
    ("_SC_THREAD_CPUTIME", sysconf::_SC_THREAD_CPUTIME),
    (
        "_SC_THREAD_PROCESS_SHARED",
        sysconf::_SC_THREAD_PROCESS_SHARED,
    ),
    (
        "_SC_THREAD_PRIORITY_SCHEDULING",
        sysconf::_SC_THREAD_PRIORITY_SCHEDULING,
    ),
    ("_SC_THREAD_PRIO_INHERIT", sysconf::_SC_THREAD_PRIO_INHERIT),
    ("_SC_THREAD_PRIO_PROTECT", sysconf::_SC_THREAD_PRIO_PROTECT),
    (
        "_SC_THREAD_ROBUST_PRIO_INHERIT",
        sysconf::_SC_THREAD_ROBUST_PRIO_INHERIT,
    ),
    (
        "_SC_THREAD_ROBUST_PRIO_PROTECT",
        sysconf::_SC_THREAD_ROBUST_PRIO_PROTECT,
    ),
    (
        "_SC_THREAD_SPORADIC_SERVER",
        sysconf::_SC_THREAD_SPORADIC_SERVER,
    ),
    ("_SC_TIMEOUTS", sysconf::_SC_TIMEOUTS),
    (
        "_SC_TYPED_MEMORY_OBJECTS",
        sysconf::_SC_TYPED_MEMORY_OBJECTS,
    ),
    ("_SC_V6_ILP32_OFF32", sysconf::_SC_V6_ILP32_OFF32),
    ("_SC_V6_ILP32_OFFBIG", sysconf::_SC_V6_ILP32_OFFBIG),
    ("_SC_V6_LP64_OFF64", sysconf::_SC_V6_LP64_OFF64),
    ("_SC_V6_LPBIG_OFFBIG", sysconf::_SC_V6_LPBIG_OFFBIG),
    ("_SC_V7_ILP32_OFF32", sysconf::_SC_V7_ILP32_OFF32),
    ("_SC_V7_ILP32_OFFBIG", sysconf::_SC_V7_ILP32_OFFBIG),
    ("_SC_V7_LP64_OFF64", sysconf::_SC_V7_LP64_OFF64),
    ("_SC_V7_LPBIG_OFFBIG", sysconf::_SC_V7_LPBIG_OFFBIG),
    ("_SC_2_PBS", sysconf::_SC_2_PBS),
    ("_SC_2_PBS_ACCOUNTING", sysconf::_SC_2_PBS_ACCOUNTING),
    ("_SC_2_PBS_CHECKPOINT", sysconf::_SC_2_PBS_CHECKPOINT),
    ("_SC_2_PBS_LOCATE", sysconf::_SC_2_PBS_LOCATE),
    ("_SC_2_PBS_MESSAGE", sysconf::_SC_2_PBS_MESSAGE),
    ("_SC_2_PBS_TRACK", sysconf::_SC_2_PBS_TRACK),
    ("_SC_TRACE", sysconf::_SC_TRACE),
    ("_SC_TRACE_EVENT_FILTER", sysconf::_SC_TRACE_EVENT_FILTER),
    (
        "_SC_TRACE_EVENT_NAME_MAX",
        sysconf::_SC_TRACE_EVENT_NAME_MAX,
    ),
    ("_SC_TRACE_INHERIT", sysconf::_SC_TRACE_INHERIT),
    ("_SC_TRACE_LOG", sysconf::_SC_TRACE_LOG),
    ("_SC_TRACE_NAME_MAX", sysconf::_SC_TRACE_NAME_MAX),
    ("_SC_TRACE_SYS_MAX", sysconf::_SC_TRACE_SYS_MAX),
    (
        "_SC_TRACE_USER_EVENT_MAX",
        sysconf::_SC_TRACE_USER_EVENT_MAX,
    ),
    ("_SC_XOPEN_STREAMS", sysconf::_SC_XOPEN_STREAMS),
    ("_SC_SS_REPL_MAX", sysconf::_SC_SS_REPL_MAX),
    ("_SC_MINSIGSTKSZ", sysconf::_SC_MINSIGSTKSZ),
    ("_SC_SIGSTKSZ", sysconf::_SC_SIGSTKSZ),
];

/// Evaluate one `_SC_*` name.
pub fn sysconf(name: i32, inputs: &SysconfInputs) -> Result<i64, Errno> {
    let value = match name {
        sysconf::_SC_ARG_MAX => arg_max(inputs),
        sysconf::_SC_CHILD_MAX => inputs.child_max,
        sysconf::_SC_CLK_TCK => i64::from(limits::CLK_TCK),
        sysconf::_SC_NGROUPS_MAX => i64::from(limits::NGROUPS_MAX),
        sysconf::_SC_OPEN_MAX => inputs.open_max,
        sysconf::_SC_STREAM_MAX => FOPEN_MAX,
        sysconf::_SC_TZNAME_MAX => POSIX_TZNAME_MAX,
        sysconf::_SC_JOB_CONTROL => POSIX_JOB_CONTROL,
        sysconf::_SC_SAVED_IDS => POSIX_SAVED_IDS,
        sysconf::_SC_VERSION => POSIX_VERSION,
        sysconf::_SC_PAGESIZE => i64::from(raven_abi::PAGE_SIZE as i32),
        sysconf::_SC_NPROCESSORS_CONF => inputs.cpus_conf,
        sysconf::_SC_NPROCESSORS_ONLN => inputs.cpus_online,
        sysconf::_SC_PHYS_PAGES => inputs.phys_pages,
        sysconf::_SC_AVPHYS_PAGES => inputs.avphys_pages,
        sysconf::_SC_IOV_MAX => i64::from(limits::IOV_MAX),
        sysconf::_SC_HOST_NAME_MAX => i64::from(limits::HOST_NAME_MAX),
        sysconf::_SC_LOGIN_NAME_MAX => i64::from(limits::LOGIN_NAME_MAX),
        sysconf::_SC_TTY_NAME_MAX => i64::from(limits::TTY_NAME_MAX),
        sysconf::_SC_SYMLOOP_MAX => i64::from(limits::SYMLOOP_MAX),
        sysconf::_SC_NZERO => 20,
        sysconf::_SC_ATEXIT_MAX => i64::MAX,
        sysconf::_SC_PASS_MAX => 128,
        SC_LEVEL1_ICACHE_SIZE
        | SC_LEVEL1_ICACHE_ASSOC
        | SC_LEVEL1_ICACHE_LINESIZE
        | SC_LEVEL1_DCACHE_SIZE
        | SC_LEVEL1_DCACHE_ASSOC
        | SC_LEVEL1_DCACHE_LINESIZE
        | SC_LEVEL2_CACHE_SIZE
        | SC_LEVEL2_CACHE_ASSOC
        | SC_LEVEL2_CACHE_LINESIZE
        | SC_LEVEL3_CACHE_SIZE
        | SC_LEVEL3_CACHE_ASSOC
        | SC_LEVEL3_CACHE_LINESIZE
        | SC_LEVEL4_CACHE_SIZE
        | SC_LEVEL4_CACHE_ASSOC
        | SC_LEVEL4_CACHE_LINESIZE => 0,
        sysconf::_SC_2_VERSION => POSIX2_VERSION,
        sysconf::_SC_2_C_BIND => POSIX2_C_BIND,
        sysconf::_SC_2_C_DEV => POSIX2_C_DEV,
        sysconf::_SC_2_CHAR_TERM => POSIX2_CHAR_TERM,
        sysconf::_SC_2_FORT_DEV | sysconf::_SC_2_FORT_RUN => -1,
        sysconf::_SC_2_LOCALEDEF => POSIX2_LOCALEDEF,
        sysconf::_SC_2_SW_DEV => POSIX2_SW_DEV,
        sysconf::_SC_2_UPE => POSIX2_UPE,
        sysconf::_SC_XOPEN_VERSION => XOPEN_VERSION,
        sysconf::_SC_XOPEN_UNIX => 1,
        sysconf::_SC_XOPEN_CRYPT => -1,
        sysconf::_SC_XOPEN_ENH_I18N => -1,
        sysconf::_SC_XOPEN_SHM => -1,
        sysconf::_SC_XOPEN_REALTIME => 1,
        sysconf::_SC_XOPEN_REALTIME_THREADS => 1,
        sysconf::_SC_XOPEN_LEGACY => -1,
        sysconf::_SC_BC_BASE_MAX => POSIX2_BC_BASE_MAX,
        sysconf::_SC_BC_DIM_MAX => POSIX2_BC_DIM_MAX,
        sysconf::_SC_BC_SCALE_MAX => POSIX2_BC_SCALE_MAX,
        sysconf::_SC_BC_STRING_MAX => POSIX2_BC_STRING_MAX,
        sysconf::_SC_COLL_WEIGHTS_MAX => POSIX2_COLL_WEIGHTS_MAX,
        sysconf::_SC_EXPR_NEST_MAX => POSIX2_EXPR_NEST_MAX,
        sysconf::_SC_LINE_MAX => POSIX2_LINE_MAX,
        sysconf::_SC_RE_DUP_MAX => POSIX_RE_DUP_MAX,
        sysconf::_SC_GETGR_R_SIZE_MAX | sysconf::_SC_GETPW_R_SIZE_MAX => 1024,
        sysconf::_SC_THREAD_DESTRUCTOR_ITERATIONS => {
            i64::from(limits::PTHREAD_DESTRUCTOR_ITERATIONS)
        }
        sysconf::_SC_THREAD_KEYS_MAX => i64::from(limits::PTHREAD_KEYS_MAX),
        sysconf::_SC_THREAD_STACK_MIN => i64::from(limits::PTHREAD_STACK_MIN),
        sysconf::_SC_THREAD_THREADS_MAX => -1,
        sysconf::_SC_THREADS => POSIX_VERSION,
        sysconf::_SC_THREAD_SAFE_FUNCTIONS => POSIX_VERSION,
        sysconf::_SC_THREAD_ATTR_STACKADDR => POSIX_VERSION,
        sysconf::_SC_THREAD_ATTR_STACKSIZE => POSIX_VERSION,
        sysconf::_SC_MONOTONIC_CLOCK => POSIX_VERSION,
        sysconf::_SC_AIO_LISTIO_MAX => 2,
        sysconf::_SC_AIO_MAX => 8,
        sysconf::_SC_AIO_PRIO_DELTA_MAX => 0,
        sysconf::_SC_DELAYTIMER_MAX => 32,
        sysconf::_SC_MQ_OPEN_MAX => 8,
        sysconf::_SC_MQ_PRIO_MAX => 32,
        sysconf::_SC_RTSIG_MAX => 32,
        sysconf::_SC_SEM_NSEMS_MAX => 256,
        sysconf::_SC_SEM_VALUE_MAX => 0x7fff_ffff,
        sysconf::_SC_SIGQUEUE_MAX => 32,
        sysconf::_SC_TIMER_MAX => 32,
        sysconf::_SC_ASYNCHRONOUS_IO => -1,
        sysconf::_SC_FSYNC => POSIX_VERSION,
        sysconf::_SC_MAPPED_FILES => POSIX_VERSION,
        sysconf::_SC_MEMLOCK => POSIX_VERSION,
        sysconf::_SC_MEMLOCK_RANGE => POSIX_VERSION,
        sysconf::_SC_MEMORY_PROTECTION => POSIX_VERSION,
        sysconf::_SC_MESSAGE_PASSING => -1,
        sysconf::_SC_PRIORITIZED_IO => -1,
        sysconf::_SC_PRIORITY_SCHEDULING => -1,
        sysconf::_SC_REALTIME_SIGNALS => -1,
        sysconf::_SC_SEMAPHORES => -1,
        sysconf::_SC_SHARED_MEMORY_OBJECTS => -1,
        sysconf::_SC_SYNCHRONIZED_IO => POSIX_VERSION,
        sysconf::_SC_TIMERS => -1,
        sysconf::_SC_ADVISORY_INFO => -1,
        sysconf::_SC_BARRIERS => -1,
        sysconf::_SC_CLOCK_SELECTION => POSIX_VERSION,
        sysconf::_SC_CPUTIME => -1,
        sysconf::_SC_IPV6 => -1,
        sysconf::_SC_RAW_SOCKETS => -1,
        sysconf::_SC_READER_WRITER_LOCKS => POSIX_VERSION,
        sysconf::_SC_REGEXP => POSIX_VERSION,
        sysconf::_SC_SHELL => POSIX_VERSION,
        sysconf::_SC_SPAWN => POSIX_VERSION,
        sysconf::_SC_SPIN_LOCKS => POSIX_VERSION,
        sysconf::_SC_SPORADIC_SERVER => -1,
        sysconf::_SC_THREAD_CPUTIME => -1,
        sysconf::_SC_THREAD_PROCESS_SHARED => -1,
        sysconf::_SC_THREAD_PRIORITY_SCHEDULING => -1,
        sysconf::_SC_THREAD_PRIO_INHERIT => -1,
        sysconf::_SC_THREAD_PRIO_PROTECT => -1,
        sysconf::_SC_THREAD_ROBUST_PRIO_INHERIT => -1,
        sysconf::_SC_THREAD_ROBUST_PRIO_PROTECT => -1,
        sysconf::_SC_THREAD_SPORADIC_SERVER => -1,
        sysconf::_SC_TIMEOUTS => POSIX_VERSION,
        sysconf::_SC_TYPED_MEMORY_OBJECTS => -1,
        sysconf::_SC_V6_ILP32_OFF32
        | sysconf::_SC_V6_ILP32_OFFBIG
        | sysconf::_SC_V6_LPBIG_OFFBIG
        | sysconf::_SC_V7_ILP32_OFF32
        | sysconf::_SC_V7_ILP32_OFFBIG
        | sysconf::_SC_V7_LPBIG_OFFBIG => -1,
        sysconf::_SC_V6_LP64_OFF64 | sysconf::_SC_V7_LP64_OFF64 => 1,
        sysconf::_SC_2_PBS
        | sysconf::_SC_2_PBS_ACCOUNTING
        | sysconf::_SC_2_PBS_CHECKPOINT
        | sysconf::_SC_2_PBS_LOCATE
        | sysconf::_SC_2_PBS_MESSAGE
        | sysconf::_SC_2_PBS_TRACK
        | sysconf::_SC_TRACE
        | sysconf::_SC_TRACE_EVENT_FILTER
        | sysconf::_SC_TRACE_EVENT_NAME_MAX
        | sysconf::_SC_TRACE_INHERIT
        | sysconf::_SC_TRACE_LOG
        | sysconf::_SC_TRACE_NAME_MAX
        | sysconf::_SC_TRACE_SYS_MAX
        | sysconf::_SC_TRACE_USER_EVENT_MAX
        | sysconf::_SC_XOPEN_STREAMS
        | sysconf::_SC_SS_REPL_MAX => -1,
        sysconf::_SC_MINSIGSTKSZ => 2048,
        sysconf::_SC_SIGSTKSZ => 16384,
        _ => return Err(Errno::EINVAL),
    };
    Ok(value)
}

fn arg_max(_inputs: &SysconfInputs) -> i64 {
    // H-19/O-3 resolved: the fixed Raven value, not the stack-derived bionic formula.
    i64::from(limits::ARG_MAX)
}

#[cfg(test)]
#[path = "sysconf_ut.rs"]
mod tests;
