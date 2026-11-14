// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The async-signal-safe set of `09` 3.8, verified against the local `signal-safety(7)`.
//!
//! `data/async-signal-safe.txt` is the authoritative list, one name per line with a trailing
//! `VERIFIED: <source>` or `UNVERIFIED: <reason>` mark. This module mirrors the names for the
//! `const fn` C64 calls; a unit test keeps the two in sync and asserts every line is marked.
//! `fork` carries the donor's note "POSIX 2017, not 2024"; `_Fork`, `sig2str` and `str2sig` are
//! the POSIX.1-2024 additions.

#![allow(clippy::indexing_slicing)] // a const fn cannot use slice::get

/// Every name, in the order of the area document.
pub const ASYNC_SAFE: &[&str] = &[
    "_Exit",
    "_exit",
    "abort",
    "accept",
    "access",
    "aio_error",
    "aio_return",
    "aio_suspend",
    "alarm",
    "bind",
    "cfgetispeed",
    "cfgetospeed",
    "cfsetispeed",
    "cfsetospeed",
    "chdir",
    "chmod",
    "chown",
    "clock_gettime",
    "close",
    "connect",
    "creat",
    "dup",
    "dup2",
    "execl",
    "execle",
    "execv",
    "execve",
    "faccessat",
    "fchdir",
    "fchmod",
    "fchmodat",
    "fchown",
    "fchownat",
    "fcntl",
    "fdatasync",
    "fexecve",
    "ffs",
    "fork",
    "fstat",
    "fstatat",
    "fsync",
    "ftruncate",
    "futimens",
    "getegid",
    "geteuid",
    "getgid",
    "getgroups",
    "getpeername",
    "getpgrp",
    "getpid",
    "getppid",
    "getsockname",
    "getsockopt",
    "getuid",
    "htonl",
    "htons",
    "kill",
    "link",
    "linkat",
    "listen",
    "longjmp",
    "lseek",
    "lstat",
    "memccpy",
    "memchr",
    "memcmp",
    "memcpy",
    "memmove",
    "memset",
    "mkdir",
    "mkdirat",
    "mkfifo",
    "mkfifoat",
    "mknod",
    "mknodat",
    "ntohl",
    "ntohs",
    "open",
    "openat",
    "pause",
    "pipe",
    "poll",
    "posix_trace_event",
    "pselect",
    "pthread_kill",
    "pthread_self",
    "pthread_sigmask",
    "raise",
    "read",
    "readlink",
    "readlinkat",
    "recv",
    "recvfrom",
    "recvmsg",
    "rename",
    "renameat",
    "rmdir",
    "select",
    "sem_post",
    "send",
    "sendmsg",
    "sendto",
    "setgid",
    "setpgid",
    "setsid",
    "setsockopt",
    "setuid",
    "shutdown",
    "sigaction",
    "sigaddset",
    "sigdelset",
    "sigemptyset",
    "sigfillset",
    "sigismember",
    "siglongjmp",
    "signal",
    "sigpause",
    "sigpending",
    "sigprocmask",
    "sigqueue",
    "sigset",
    "sigsuspend",
    "sleep",
    "sockatmark",
    "socket",
    "socketpair",
    "stat",
    "stpcpy",
    "stpncpy",
    "strcat",
    "strchr",
    "strcmp",
    "strcpy",
    "strcspn",
    "strlen",
    "strncat",
    "strncmp",
    "strncpy",
    "strnlen",
    "strpbrk",
    "strrchr",
    "strspn",
    "strstr",
    "tcdrain",
    "tcflow",
    "tcflush",
    "tcgetattr",
    "tcgetpgrp",
    "tcsendbreak",
    "tcsetattr",
    "tcsetpgrp",
    "time",
    "timer_getoverrun",
    "timer_gettime",
    "timer_settime",
    "times",
    "umask",
    "uname",
    "unlink",
    "unlinkat",
    "utime",
    "utimensat",
    "utimes",
    "wait",
    "waitpid",
    "wcpcpy",
    "wcpncpy",
    "wcscat",
    "wcschr",
    "wcscmp",
    "wcscpy",
    "wcscspn",
    "wcslen",
    "wcsncat",
    "wcsncmp",
    "wcsncpy",
    "wcsnlen",
    "wcspbrk",
    "wcsrchr",
    "wcsspn",
    "wcsstr",
    "wmemchr",
    "wmemcmp",
    "wmemcpy",
    "wmemmove",
    "wmemset",
    "write",
    "_Fork",
    "sig2str",
    "str2sig",
];

/// The functions this crate exports (or the shell exports from it) that the list names.
pub const CRATE_FUNCTIONS: &[&str] = &[
    "sigaction",
    "sigaddset",
    "sigdelset",
    "sigemptyset",
    "sigfillset",
    "sigismember",
    "signal",
    "sigpause",
    "sigpending",
    "sigprocmask",
    "sigqueue",
    "sigset",
    "sigsuspend",
    "raise",
    "kill",
    "longjmp",
    "siglongjmp",
    "sig2str",
    "str2sig",
    "pthread_sigmask",
];

/// `const fn` string equality (no allocation, no formatting).
pub const fn eq_bytes(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0usize;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index = index.wrapping_add(1);
    }
    true
}

/// Whether `name` is on the async-signal-safe list.
#[must_use]
pub const fn is_async_signal_safe(name: &str) -> bool {
    let bytes = name.as_bytes();
    let mut index = 0usize;
    while index < ASYNC_SAFE.len() {
        if eq_bytes(bytes, ASYNC_SAFE[index].as_bytes()) {
            return true;
        }
        index = index.wrapping_add(1);
    }
    false
}

#[cfg(test)]
#[path = "safe_list_ut.rs"]
mod safe_list_ut;
