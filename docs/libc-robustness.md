<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc robustness: adversarial arguments

`tests/libc/robust` is a host nextest crate that feeds deterministic garbage to the wrapper path used
by the login and SSH server programs. The C-ABI exports are target-only because their names collide
with the host libc (`libc/ravenlibc/crates/raven-libc/src/lib.rs`), so the host rows drive the same
`raven-libc-platform` and pure-logic function that each shell wrapper forwards to, using the
deterministic `FakeSys` kernel. Every row asserts the documented return/errno, that the test process
is still alive, and that a legal follow-up call succeeds.

```
timeout 300 cargo nextest run -p raven-libc-robust
```

Result at K460: **30/30 passed** (`/tmp/opencode/k460-green3.log`).

## Wrapper x vector table

| # | Wrapper | Adversarial vector | Expected | Actual | Shell -> implementation |
|---:|---|---|---|---|---|
| 1 | `read` | fd `-1`, `INT_MAX` | `EBADF` | `EBADF` | `unistd/read.rs:13` -> `platform.rs:776` |
| 2 | `write` | fd `-1`, `INT_MAX` | `EBADF` | `EBADF` | `unistd/write.rs:13` -> `platform.rs:825` |
| 3 | `open` | empty, 4097-byte, embedded NUL | `ENOENT`, `ENAMETOOLONG`, `EINVAL` | match | `platform/open.rs:7` -> `platform.rs:745` |
| 4 | `openat` | `dirfd` `-1`, `INT_MAX` | `EBADF` | `EBADF` | `platform.rs:687` |
| 5 | `fstat` | fd `-1`, `INT_MAX` | `EBADF` | `EBADF` | `platform/fstat.rs:13` -> `platform.rs:1012` |
| 6 | `close` | fd `-1`, `INT_MAX`, double close | `EBADF` | `EBADF` | `unistd/close.rs:8` -> `platform.rs:471` |
| 7 | `dup` | fd `-1`, `INT_MAX`; `min_fd = u32::MAX` | `EBADF`; `EINVAL` | match | `unistd/dup.rs:8` -> `platform.rs:412` |
| 8 | `dup2` | old `-1`; new `-1`, `INT_MAX` | `EBADF` | `EBADF` | `platform.rs:433` |
| 9 | `fcntl` | fd `-1`, `INT_MAX`; cmd `INT_MIN`; `F_DUPFD -1` | `EBADF`; `EINVAL` | match | `unistd/fcntl.rs:4` -> `platform.rs:614` |
| 10 | `poll` | 4097 entries; fd `-1`; fd `INT_MAX` | `EINVAL`; `0`; `1`+`POLLNVAL` | match | `platform/poll.rs:13` -> `poll/poll.rs:36` |
| 11 | `ppoll` | `tv_sec -1`; `tv_nsec -1`; `tv_nsec 1e9` | `EINVAL` | was `Ok(0)`; **fixed** | `p1c_exports/ppoll.rs:13` -> `poll/poll.rs:355` |
| 12 | `select` | `nfds -1`, `INT_MAX`; `tv_usec 1e6` | `EINVAL` | match | `p1c_exports/select.rs:13` -> `poll/select.rs:35` |
| 13 | `mmap` | len `0`/`SIZE_MAX`; offset `-1`/`1`; prot `0x80`; W\|X; both types; no type | `EINVAL`/`ENOMEM`/`EACCES` | no-type was `Ok(addr)`; **fixed** | `platform/mmap.rs:11` -> `mem/mmap.rs:52` |
| 14 | `munmap` | len `0`; unaligned address | `EINVAL` | match | `platform/munmap.rs:11` -> `mem/mmap.rs:122` |
| 15 | `mprotect` | len `0`; unaligned address; prot `0x80`; W\|X | `EINVAL`/`EACCES` | match | `platform/mprotect.rs:11` -> `mem/mmap.rs:134` |
| 16 | `lseek` | fd `-1`, `INT_MAX`; whence `INT_MAX` | `EBADF`; `EINVAL` | match | `platform.rs:994` |
| 17 | `pipe2` | flags `INT_MIN`, `-1` | `EINVAL` | match | `platform.rs:1888` |
| 18 | `getcwd` | empty buffer | `ERANGE` | match | `platform.rs:1531` |
| 19 | `stat` | empty, 4097-byte, embedded NUL | `ENOENT`, `ENAMETOOLONG`, `EINVAL` | match | `platform.rs:1024` |
| 20 | `chdir` | empty, 4097-byte, embedded NUL | `ENOENT`, `ENAMETOOLONG`, `EINVAL` | match | `platform.rs:1486` |
| 21 | `strtol` | empty, `"-"`; bases `1`, `-1`, `37`, `INT_MIN`, `INT_MAX` | `NoDigits`; `InvalidBase` | match | `stdlib/strtol.rs:11` -> `stdlib/conv/strtol.rs` |
| 22 | `strtoul` | empty; bases `1`, `-1`, `37` | `NoDigits`; `InvalidBase` | match | `stdlib/strtoul.rs:11` -> `stdlib/conv/strtoul.rs` |
| 23 | `qsort` | zero elements; element size `0` | no panic; input unchanged | match | `stdlib/qsort.rs:13` -> `stdlib/sort/qsort.rs:284` |
| 24 | `getenv` | empty name; name containing `=` | `NULL` | `None` | `stdlib/getenv.rs:11` -> `stdlib/env/getenv.rs:21` |
| 25 | `setenv` | empty name; name containing `=` | `EINVAL` | `EINVAL` | `stdlib/setenv.rs:11` -> `stdlib/env/setenv.rs:52` |
| 26 | `realpath` | out `< PATH_MAX`; empty path; 5000-byte path | `EINVAL`; `ENOENT`; `ENAMETOOLONG` | match | `stdlib/realpath.rs:162` -> `stdlib/path/realpath.rs:82` |
| 27 | `snprintf` | empty format; trailing `%`; `%s` with NULL; cap `SIZE_MAX` | `0`; `0`; `6`; `Overflow` | match, no panic | `stdio/trampolines.rs:13` -> `fmt/glue/snprintf.rs:26` |
| 28 | `sigaction` | sig `0`, `-1`, `32`, `33`, `34`, `65`, `INT_MAX`; `SIGKILL`/`SIGSTOP` install | `EINVAL` | match | `signal/sigaction.rs:25` -> `signal/action.rs:50` |
| 29 | `sigprocmask` | `how` `3`, `-1`, `INT_MIN`, `INT_MAX` | `EINVAL` | match | `signal/sigprocmask.rs:14` -> `signal/maskops.rs:47` |
| 30 | `waitpid` | pid `INT_MAX` (fake `ECHILD`), `INT_MIN`/options `u32::MAX`, `-1`, `42` | pass-through, no panic | match | `process/waitpid.rs:13` -> `proc/waitpid.rs:39` |
| 31 | `recv`/`send` | flags `INT_MIN`, unknown `0x0800_0000`, `MSG_OOB` | `EINVAL`; `EOPNOTSUPP` | match | `socket/recv.rs:16`, `socket/socketpair.rs:43` -> `net/flags.rs` |

## Deviations and open scope

1. **`ppoll` timeout** (`libc/ravenlibc/crates/raven-libc-platform/src/poll/poll.rs:355`): a negative
   `tv_sec`/`tv_nsec` or `tv_nsec >= 1_000_000_000` used to collapse to an immediate `Ok(0)`. POSIX
   requires `EINVAL` for an invalid timespec. Fixed at `poll.rs:364`.
2. **`mmap` map type** (`libc/ravenlibc/crates/raven-libc-platform/src/mem/mmap.rs:28`): flags with
   neither `MAP_SHARED` nor `MAP_PRIVATE` used to reach the backend and returned a mapping. POSIX
   requires exactly one map type and `EINVAL` otherwise. Fixed at `mmap.rs:48`.
3. **`snprintf` trailing `%`** (`raven-libc-fmt/src/glue/snprintf.rs:26`): the engine stops at the
   implicit NUL and returns `0` without writing. A trailing conversion specifier is undefined by
   ISO C/POSIX; the row's contract is "no panic, no out-of-bounds write". The engine is outside this
   ticket's Owns, so the behaviour is recorded, not changed.
4. **`snprintf` `%s` with `NULL`** renders the common `"(null)"` (6 bytes) without dereferencing the
   pointer. Also undefined by C; recorded as the engine's policy.
5. **`pipe2` creation** needs `RealSys` plus the `host-fake` syscall model; with `FakeSys` the
   `raven-libc-sys` pipe handles are not in the fake world. The row therefore proves the flag
   rejection (`EINVAL`) and uses `getcwd` as the follow-up success.
6. **`sigprocmask` valid `how`** writes the thread control block and is covered by the signal
   crate's own TCB tests; this host row covers the invalid-`how` rejection before the pointer is
   touched.
7. **`recv`/`send` flag validation** lives in `raven-libc-net` (outside K460 Owns); the row drives
   that exact validator. The C shell forwards to it after the pointer checks.
8. **`waitpid`** is a pass-through to the kernel seam; the fake proves the pid/options path and that
   no wrapper-side panic occurs. Kernel-side validation is K456's scope.

No kernel edits, no C, no `tests/ssh`, `tests/libc/direct` or `tests/smp/syscall-fuzz` change.
