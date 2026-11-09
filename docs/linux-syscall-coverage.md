<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Linux syscall coverage

Generated: 2026-11-09
Host: 5.19.0-50-generic (x86_64)

Programs: 52 total, 1 skipped, 0 not passing.

## Top 60 syscalls by number of programs

| # | syscall | x86_64 nr | programs | calls | errors |
|---|---|---|---|---|---|
| 1 | `arch_prctl` | 158 | 51 | 142 | 68 |
| 2 | `execve` | 59 | 51 | 86 | 12 |
| 3 | `exit_group` | 231 | 51 | 70 | 0 |
| 4 | `brk` | 12 | 49 | 386 | 0 |
| 5 | `mprotect` | 10 | 49 | 918 | 0 |
| 6 | `set_tid_address` | 218 | 49 | 72 | 0 |
| 7 | `close` | 3 | 48 | 2745 | 2 |
| 8 | `mmap` | 9 | 48 | 1908 | 0 |
| 9 | `write` | 1 | 48 | 205 | 1 |
| 10 | `prlimit64` | 302 | 47 | 99 | 0 |
| 11 | `munmap` | 11 | 46 | 245 | 0 |
| 12 | `newfstatat` | 262 | 46 | 7305 | 3083 |
| 13 | `read` | 0 | 46 | 3182 | 0 |
| 14 | `openat` | 257 | 45 | 6536 | 4269 |
| 15 | `rseq` | 334 | 45 | 118 | 0 |
| 16 | `set_robust_list` | 273 | 45 | 128 | 0 |
| 17 | `getrandom` | 318 | 44 | 106 | 0 |
| 18 | `access` | 21 | 42 | 348 | 273 |
| 19 | `pread64` | 17 | 42 | 293 | 0 |
| 20 | `ioctl` | 16 | 38 | 498 | 492 |
| 21 | `rt_sigaction` | 13 | 35 | 1022 | 2 |
| 22 | `lseek` | 8 | 29 | 670 | 5 |
| 23 | `futex` | 202 | 25 | 2332 | 90 |
| 24 | `fcntl` | 72 | 23 | 714 | 0 |
| 25 | `getuid` | 102 | 21 | 68 | 0 |
| 26 | `geteuid` | 107 | 19 | 80 | 0 |
| 27 | `getgid` | 104 | 19 | 64 | 0 |
| 28 | `getegid` | 108 | 17 | 62 | 0 |
| 29 | `readlink` | 89 | 17 | 2275 | 2228 |
| 30 | `getdents64` | 217 | 16 | 430 | 0 |
| 31 | `sysinfo` | 99 | 16 | 21 | 0 |
| 32 | `getcwd` | 79 | 15 | 59 | 0 |
| 33 | `rt_sigprocmask` | 14 | 15 | 345 | 0 |
| 34 | `getpid` | 39 | 13 | 56 | 0 |
| 35 | `sigaltstack` | 131 | 13 | 142 | 0 |
| 36 | `statfs` | 137 | 13 | 26 | 26 |
| 37 | `connect` | 42 | 12 | 33 | 30 |
| 38 | `socket` | 41 | 12 | 39 | 0 |
| 39 | `uname` | 63 | 12 | 22 | 0 |
| 40 | `clone` | 56 | 9 | 23 | 0 |
| 41 | `getppid` | 110 | 8 | 10 | 0 |
| 42 | `gettid` | 186 | 8 | 64 | 0 |
| 43 | `pipe2` | 293 | 8 | 15 | 0 |
| 44 | `poll` | 7 | 8 | 13 | 0 |
| 45 | `prctl` | 157 | 8 | 38 | 0 |
| 46 | `sched_getaffinity` | 204 | 8 | 59 | 0 |
| 47 | `statx` | 332 | 7 | 225 | 26 |
| 48 | `wait4` | 61 | 7 | 29 | 9 |
| 49 | `epoll_create1` | 291 | 6 | 7 | 0 |
| 50 | `rt_sigreturn` | 15 | 6 | 16 | 0 |
| 51 | `dup` | 32 | 5 | 15 | 0 |
| 52 | `dup2` | 33 | 5 | 14 | 0 |
| 53 | `epoll_ctl` | 233 | 5 | 10 | 4 |
| 54 | `eventfd2` | 290 | 5 | 6 | 0 |
| 55 | `fadvise64` | 221 | 5 | 6 | 2 |
| 56 | `madvise` | 28 | 5 | 69 | 0 |
| 57 | `unlink` | 87 | 5 | 22 | 5 |
| 58 | `clone3` | 435 | 4 | 50 | 0 |
| 59 | `epoll_wait` | 232 | 4 | 5 | 0 |
| 60 | `exit` | 60 | 4 | 12 | 0 |

## Programs by category

### static

| program | status | exit | syscalls | reason |
|---|---|---|---|---|
| `busybox-ls` | ok | 0 | 25 |  |
| `busybox-sh-pipe` | ok | 0 | 31 |  |
| `files-rust-musl` | ok | 0 | 23 |  |
| `hello-c-glibc-static` | ok | 0 | 15 |  |
| `hello-go-static` | ok | 0 | 20 |  |
| `hello-rust-musl` | ok | 0 | 13 |  |
| `net-loopback-rust-musl` | ok | 0 | 27 |  |
| `threads-rust-musl` | ok | 0 | 18 |  |

### dynamic

| program | status | exit | syscalls | reason |
|---|---|---|---|---|
| `awk-sum` | ok | 0 | 33 |  |
| `bash-arith-loop` | ok | 0 | 34 |  |
| `cat-os-release` | ok | 0 | 20 |  |
| `cp-file` | ok | 0 | 23 |  |
| `curl-file-url` | ok | 0 | 31 |  |
| `curl-version` | ok | 0 | 33 |  |
| `date-utc` | ok | 0 | 20 |  |
| `find-etc` | ok | 0 | 26 |  |
| `git-version` | ok | 0 | 22 |  |
| `grep-passwd` | ok | 0 | 22 |  |
| `grep-recursive` | ok | 0 | 39 |  |
| `gzip-stdout` | ok | 0 | 19 |  |
| `id-self` | ok | 0 | 29 |  |
| `jq-version` | ok | 0 | 20 |  |
| `ls-la` | ok | 0 | 29 |  |
| `ls-root` | ok | 0 | 30 |  |
| `sed-substitute` | ok | 0 | 22 |  |
| `sh-pipeline` | ok | 0 | 40 |  |
| `sort-file` | ok | 0 | 30 |  |
| `ssh-version` | ok | 0 | 28 |  |
| `stat-etc` | ok | 0 | 25 |  |
| `tar-list` | ok | 0 | 26 |  |
| `uname-a` | ok | 0 | 21 |  |
| `wc-etc-passwd` | ok | 0 | 21 |  |

### service

| program | status | exit | syscalls | reason |
|---|---|---|---|---|
| `openssl-version` | ok | 0 | 23 |  |
| `python3-epoll-echo` | ok | 0 | 44 |  |
| `python3-sqlite` | ok | 0 | 37 |  |
| `sqlite3-version` | skipped | - | 0 | not installed: sqlite3 |
| `ssh-keygen-ed25519` | ok | 0 | 29 |  |

### language

| program | status | exit | syscalls | reason |
|---|---|---|---|---|
| `go-version` | ok | 0 | 29 |  |
| `perl-print` | ok | 0 | 28 |  |
| `python3-print` | ok | 0 | 32 |  |
| `python3-threads` | ok | 0 | 38 |  |
| `ruby-print` | ok | 0 | 40 |  |

### toolchain

| program | status | exit | syscalls | reason |
|---|---|---|---|---|
| `cargo-version` | ok | 0 | 41 |  |
| `gcc-compile-object` | ok | 0 | 31 |  |
| `gcc-version` | ok | 0 | 22 |  |
| `git-init-commit` | ok | 0 | 48 |  |
| `make-version` | ok | 0 | 24 |  |
| `rustc-compile-hello` | ok | 0 | 64 |  |
| `rustc-version` | ok | 0 | 44 |  |

### shell

| program | status | exit | syscalls | reason |
|---|---|---|---|---|
| `dash-shell` | ok | 0 | 27 |  |
| `sh-mv-rm-workflow` | ok | 0 | 35 |  |

### net

| program | status | exit | syscalls | reason |
|---|---|---|---|---|
| `python3-net-loopback` | ok | 0 | 42 |  |
