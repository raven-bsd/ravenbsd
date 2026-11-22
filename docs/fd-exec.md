<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Descriptors across exec (K486)

`tests/smp/fd-exec` is a C guest suite under the `libc-test` profile. The one-shot client opens a
known fd set, marks some descriptors `FD_CLOEXEC`, calls `execve` on itself in `child` mode and the
child probes every number with `fcntl(F_GETFD)`. The parent then checks its own open-fd count
against the baseline. The suite also covers `fcntl(F_SETFD)` after `open` and `dup`, ten rounds of
64 descriptors, `closefrom`, and a socketpair row that is informational for K482.

```
timeout 300 cargo xtask qtest --case tests/smp/fd-exec/fd-exec.toml --arch x86_64
```

The case file lives in the suite (the `xtask` resolver also accepts a path) and uses the
`libc-test` profile so `raven-init` supplies stdio; a bare `/init` would not have fd 1.

Measured on the merged K486 branch: **pass**, `duration_ms=98772` (first uncached pass
169712 ms), exit status 33, `svcd: settled`, `init exited (0)`.

## CLOEXEC matrix

| Setup | fd | After exec | Evidence |
|---|---|---:|---|
| `open(SELF, O_RDONLY \| O_CLOEXEC)` | src | absent (`EBADF`) | child `close` list |
| `open(SELF, O_RDONLY)` | src2 | open, readable | child `keep` list |
| `dup(src)` (src is CLOEXEC) | d1 | open, readable | plain dup does not copy the flag |
| `fcntl(src, F_DUPFD_CLOEXEC, 0)` | d2 | absent | the dup itself is CLOEXEC |
| `dup(src2)` then `fcntl(d3, F_SETFD, FD_CLOEXEC)` | d3 | absent | only d3 is marked |
| 64 opens, even indices marked `F_SETFD FD_CLOEXEC` | 32 even / 32 odd | 32 absent / 32 open | child per-number probe |

The child checks `fcntl(fd, F_GETFD) == -1 && errno == EBADF` for every close-list fd, and
`fcntl(fd, F_GETFD) != -1` plus a one-byte `read` for every keep-list fd. The socket fd is kept but
probed with `getsockopt` instead of `read`.

## Round table

Every round opens 64 fds, adds the five dup/`F_SETFD` variants, execs and waits:

| Round | keep | close | parent after | baseline | result |
|---:|---:|---:|---:|---:|---|
| 1 | 34 | 35 | 3 | 3 | ok |
| 2 | 34 | 35 | 3 | 3 | ok |
| 3 | 34 | 35 | 3 | 3 | ok |
| 4 | 34 | 35 | 3 | 3 | ok |
| 5 | 34 | 35 | 3 | 3 | ok |
| 6 | 34 | 35 | 3 | 3 | ok |
| 7 | 34 | 35 | 3 | 3 | ok |
| 8 | 34 | 35 | 3 | 3 | ok |
| 9 | 34 | 35 | 3 | 3 | ok |
| 10 | 34 | 35 | 3 | 3 | ok |

The parent's count returns to 3 every round; there is no growth. The child prints its own
`child ok=1 keep=1 close=1` for each round.

## `closefrom(8)`

A forked child opens 12 fds (numbers 3..14) and marks fds 10..14 `FD_CLOEXEC`. After
`closefrom(8)`, fds 3..7 are still open and fds 8..14 are `EBADF`; the CLOEXEC marking does not
protect an fd from `closefrom`. The parent checks the child's exit code and prints
`fd-exec: closefrom ok`.

## Socketpair row (K482 finding)

The socketpair fds are created without `SOCK_CLOEXEC`, so the number survives `execve` (the child's
`fcntl(F_GETFD)` succeeds). The socket **kind** does not: the child's
`getsockopt(fd, SOL_SOCKET, SO_TYPE)` returns `ENOTSOCK` (errno 88), and the row prints
`fd-exec: socketpair ok=0`.

The platform restores the OFD kind (`libc/ravenlibc/crates/raven-libc-platform/src/platform.rs:1832`
`restore_fdmap`), but the net crate's per-process `SocketRec` table is not rebuilt; `getsockopt`
looks up that table:

```
libc/ravenlibc/crates/raven-libc-net/src/socket.rs:852  pub fn getsockopt(...)
libc/ravenlibc/crates/raven-libc-net/src/socket.rs:862      let rec = table.get_mut(info.ofd).ok_or(Errno::ENOTSOCK)?;
```

This is K482's socket-inheritance work, not a K486 fd-table defect: the descriptor number and the
`fcntl` contract survive. The suite treats the row as informational (`socketpair ok=[01]`) and the
core `done ok=1` is the K486 exit condition. Do not edit `libc/.../socket/**` here.

## Mutation

The suite's own negative check changed `closefrom(8)` to `closefrom(9)`. The child left fd 8 open,
printed `fd-exec: closefrom FAIL`, the parent printed `fd-exec: done ok=0`, and the qtest failed
with the unmatched `closefrom ok`/`done ok=1` expectations. The threshold was restored; the suite
is green. This proves the closefrom row is sensitive to an off-by-one, not a vacuous assertion.

No product fd-path code was changed by K486: the core CLOEXEC/dup/closefrom/no-leak rows pass on
the base. The only disagreement is the socket record, owned by K482.

## Donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| close-on-exec sweep | Illumos `exec` closes `FD_CLOEXEC` descriptors in the fd table; CDDL behaviour only | FreeBSD `sys/kern/kern_descrip.c:2961` `fdcloseexec` (BSD-2-Clause), called from `sys/kern/kern_exec.c:867`; `fdcopy`/`fdfree` in the same file | `do_close_on_exec` outcome only | Raven's libc `close_cloexec_fds` and `exec_fdmap_env` are the existing Rust implementation; the suite verifies the contract, no new code |
| `closefrom` | Illumos/POSIX `closefrom` semantics: every fd `>= lowfd` | NetBSD `lib/libc/gen/closefrom.c` (BSD-3-Clause) is `fcntl(F_CLOSEM)`; FreeBSD `lib/libc/sys/closefrom.c` (BSD-2-Clause) is `close_range(MAX(0, lowfd), ~0U, 0)`; OpenBSD `lib/libc/sys/w_closefrom.c` | `close_range` outcome only | Raven's `raven-libc-personality-bsd/src/closefrom.rs` is the existing clean implementation; the suite verifies the exact threshold |
| inherited socket kind | Illumos keeps the socket object across exec | FreeBSD `kern_descrip.c` keeps the file object; the scheme socket record is Raven-specific | outcome only | The descriptor survives; the Raven net record table is not restored. Report for K482, do not edit its socket files |

Donor revisions: freebsd `133344405f532bd54ff3629a77d4c4e062f01fdb`, netbsd
`36eddebcc36cb0775aa2962005b9c05419a6ff01`, openbsd
`4ce71f2817f675745b0596aaf04de4a122b1cc52` (`donors.lock`). No donor code was copied.

## Gates

| Command | Cap | Exit | Result |
|---|---:|---:|---|
| `cargo xtask qtest --case tests/smp/fd-exec/fd-exec.toml --arch x86_64` | 300 | 0 | pass, 98.8 s (first uncached 169.7 s) |
| `make check-fast` | 900 | 0 | **check-fast: PASSED** (97.39 s) |

No kernel, libc or userland behaviour was changed by K486.
