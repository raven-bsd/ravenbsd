// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The data table of every C-variadic export of ravenlibc: `for_each_variadic!`.
//!
//! One callback invocation per row of C59 section 4.6, with the four row fields plus the
//! prefix. The callback decides what to do with them: the shell (C60) expands
//! `raven_variadic!` once per row with the real `v*` targets, and
//! the host test crate does the same with `"t_"` and recorder targets, so a test symbol never
//! collides with the host libc.
//!
//! Adding a row is a one-line change here plus the target in the owning family; the
//! completeness test compares this table with the API census when the census carries the
//! `variadic` field.
//!
//! Row ownership (the table file is C59's; the targets belong to the family in the last
//! column of the ticket's table): stdio and wide stdio (C60), `err`/`warn` (C41), `syslog`
//! (C60), `open`/`openat`/`fcntl`/`ioctl`/`semctl`/`mq_open`/`sem_open`/`syscall` (C55),
//! `execl*` (C53/C60), `prctl` (C66), `ptrace` (refusal stub), `strfmon`/`strfmon_l` (C48),
//! `mremap` (C49/C55), `makecontext` (C58).

/// The number of rows of the table: every variadic export of C59 section 4.6.
pub const VARIADIC_ROWS: usize = 40;

/// Invoke `$cb` once per row of the variadic table, with `$prefix` prepended to every
/// exported name.
///
/// Each invocation has the form
/// `$cb! { prefix: <prefix>, export: <name>, target: <ident>, named: <count> }`, where
/// `named` is the number of fixed leading arguments the target receives before the
/// `va_list`.
#[macro_export]
macro_rules! for_each_variadic {
    ($cb:ident, $prefix:literal) => {
        $cb! { prefix: $prefix, export: "printf", target: vprintf, named: 1 }
        $cb! { prefix: $prefix, export: "fprintf", target: vfprintf, named: 2 }
        $cb! { prefix: $prefix, export: "sprintf", target: vsprintf, named: 2 }
        $cb! { prefix: $prefix, export: "snprintf", target: vsnprintf, named: 3 }
        $cb! { prefix: $prefix, export: "dprintf", target: vdprintf, named: 2 }
        $cb! { prefix: $prefix, export: "asprintf", target: vasprintf, named: 2 }
        $cb! { prefix: $prefix, export: "scanf", target: vscanf, named: 1 }
        $cb! { prefix: $prefix, export: "fscanf", target: vfscanf, named: 2 }
        $cb! { prefix: $prefix, export: "sscanf", target: vsscanf, named: 2 }
        $cb! { prefix: $prefix, export: "wprintf", target: vwprintf, named: 1 }
        $cb! { prefix: $prefix, export: "fwprintf", target: vfwprintf, named: 2 }
        $cb! { prefix: $prefix, export: "swprintf", target: vswprintf, named: 3 }
        $cb! { prefix: $prefix, export: "wscanf", target: vwscanf, named: 1 }
        $cb! { prefix: $prefix, export: "fwscanf", target: vfwscanf, named: 2 }
        $cb! { prefix: $prefix, export: "swscanf", target: vswscanf, named: 2 }
        $cb! { prefix: $prefix, export: "snprintf_ss", target: vsnprintf_ss, named: 3 }
        $cb! { prefix: $prefix, export: "err", target: verr, named: 2 }
        $cb! { prefix: $prefix, export: "errx", target: verrx, named: 2 }
        $cb! { prefix: $prefix, export: "warn", target: vwarn, named: 1 }
        $cb! { prefix: $prefix, export: "warnx", target: vwarnx, named: 1 }
        $cb! { prefix: $prefix, export: "errc", target: verrc, named: 3 }
        $cb! { prefix: $prefix, export: "warnc", target: vwarnc, named: 2 }
        $cb! { prefix: $prefix, export: "syslog", target: vsyslog, named: 2 }
        $cb! { prefix: $prefix, export: "open", target: __raven_va_open, named: 2 }
        $cb! { prefix: $prefix, export: "openat", target: __raven_va_openat, named: 3 }
        $cb! { prefix: $prefix, export: "fcntl", target: __raven_va_fcntl, named: 2 }
        $cb! { prefix: $prefix, export: "ioctl", target: __raven_va_ioctl, named: 2 }
        $cb! { prefix: $prefix, export: "execl", target: __raven_va_execl, named: 2 }
        $cb! { prefix: $prefix, export: "execle", target: __raven_va_execle, named: 2 }
        $cb! { prefix: $prefix, export: "execlp", target: __raven_va_execlp, named: 2 }
        $cb! { prefix: $prefix, export: "semctl", target: __raven_va_semctl, named: 3 }
        $cb! { prefix: $prefix, export: "prctl", target: __raven_va_prctl, named: 1 }
        $cb! { prefix: $prefix, export: "ptrace", target: __raven_va_ptrace, named: 1 }
        $cb! { prefix: $prefix, export: "mq_open", target: __raven_va_mq_open, named: 2 }
        $cb! { prefix: $prefix, export: "sem_open", target: __raven_va_sem_open, named: 2 }
        $cb! { prefix: $prefix, export: "syscall", target: __raven_va_syscall, named: 1 }
        $cb! { prefix: $prefix, export: "strfmon", target: __raven_va_strfmon, named: 3 }
        $cb! { prefix: $prefix, export: "strfmon_l", target: __raven_va_strfmon_l, named: 4 }
        $cb! { prefix: $prefix, export: "mremap", target: __raven_va_mremap, named: 4 }
        $cb! { prefix: $prefix, export: "makecontext", target: __raven_va_makecontext, named: 3 }
    };
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "table_ut.rs"]
mod tests;
