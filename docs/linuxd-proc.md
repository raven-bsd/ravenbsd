<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# linuxd synthetic `/proc` and `/sys`

Status: B30, 2026-11-15. Evidence tags: **OBSERVED** (read or measured on the target),
**PROPOSED** (not implemented), **UNVERIFIED** (not exercised).

## 1. Why

Common Linux programs read a small procfs/sysfs surface before doing useful work. B20 runs a static
hello; B30 adds the synthetic files `linuxd` can serve without a filesystem server, a kernel
personality or a second syscall path. The values come from the Linux process context and the fixed
M5 machine description; nothing is copied from the host. This is a userspace personality server
feature (`architecture/09-linux-compat.md`); no `raven-abi` or kernel edit.

## 2. Paths and content

| Path | Kind | Content |
|---|---|---|
| `/proc` | dir | `self`, `cpuinfo`, `meminfo` |
| `/proc/self` | dir | `exe`, `maps`, `fd`, `status`, `cmdline`, `stat` |
| `/proc/self/exe` | symlink | the loaded absolute path (`/hello` in the qtest) |
| `/proc/self/maps` | file | one line per loader region: `start-end rw-p 00000000 00:00 0 [anon]` |
| `/proc/self/fd` | dir | `.`, `..`, `0`, `1`, `2` and every open synthetic fd as symlinks |
| `/proc/self/fd/<n>` | symlink | `/dev/null` (0), `tty:console` (1, 2) or the opened path |
| `/proc/self/status` | file | `Name`, `Umask`, `State`, `Tgid`, `Pid`, `PPid`, `Uid`, `Gid`, `Groups`, `Vm*`, `Threads`, `Sig*` |
| `/proc/self/cmdline` | file | NUL-separated argv, trailing NUL |
| `/proc/self/stat` | file | 52 whitespace fields; pid, `(comm)`, state `R`, `start_brk`, vsize/rss from the mappings |
| `/proc/cpuinfo` | file | one processor block with `vendor_id`, `model name`, `cpu MHz`, flags |
| `/proc/meminfo` | file | `MemTotal`, `MemFree`, `MemAvailable`, `Buffers`, `Cached`, `Swap*` |
| `/sys/devices/system/cpu/online` | file | `0\n` (the M5 one-CPU machine description) |
| `/sys`, `/sys/devices`, `/sys/devices/system`, `/sys/devices/system/cpu` | dirs | the path to `online` |

Values are deterministic: the pid comes from the Linux process table, `comm`/`exe`/`cmdline` from
the loaded image and argv, `maps`/`stat` from the loader regions and break, and `cpuinfo`/`meminfo`/
`online` are the fixed M5 virtual-machine description (one CPU, 256 MiB memory). `stat` field 52 is
present so a standard parser sees a complete record; fields that have no native meaning are zero.

## 3. Syscalls added

`open`, `openat`, `read`, `close`, `lseek`, `readlink`, `readlinkat`, `getdents64` (in addition to
the B20 `write`/`exit_group`/`brk`/`mmap`/`arch_prctl`/`getpid`/`uname`/`rt_sigprocmask`). They are
handlers in `linuxd`, not a kernel change.

* Paths are absolute; `openat` accepts `AT_FDCWD` (or any dirfd for an absolute path). The trap loop
  copies a bounded 128-byte path window; a longer path is not served.
* Read-only: `O_RDONLY` with `O_CLOEXEC`/`O_NONBLOCK`/`O_DIRECTORY`; write access is `EACCES`, other
  flags `EINVAL`. Opening a symlink follows nothing (`ELOOP`); use `readlink`/`readlinkat`.
* `read`/`readlink`/`readlinkat`/`getdents64` return their bytes through the trap loop's existing
  copy-out path. Directory reads are `EISDIR`; `getdents64` emits Linux `linux_dirent64` records.
* `lseek` supports `SEEK_SET`/`SEEK_CUR`/`SEEK_END` on regular files.
* The Linux stack is now mapped with a 256 KiB reserve below the initial image (the B20 one-page
  stack faulted on a normal stack frame); the image remains at the top.

Not implemented (documented limits):

* `fstat`/`newfstatat`/`stat` are not served, so a program that stats a synthetic file (musl
  `opendir`/`fstat`) still gets `-ENOSYS`; the qtest uses the raw syscall set above.
* Only `/proc/self`; no `/proc/<pid>`, `/proc/self/limits`, `/proc/mounts`, `/proc/self/status`
  optional fields, IPv6, or writeable proc files.
* One Linux process per server (inherited from B20: the trap packet carries no sender identity).

## 4. Tests and corpus

`tests/qtest/cases/linux-proc.toml` boots the `linux-hello` profile with a prebuilt initfs:
`/init` is `linuxd`, `/hello` is the static musl probe `tests/fixtures/linuxd/proc.c`. The probe
reads every file with raw syscalls and asserts the shape; the qtest matches one line per file, the
`/proc/self/fd` directory, the `exe` target, the `ENOENT` negative case and `linuxd: hello exit=0`.

The B05 corpus has no row whose syscalls are all in the current B20+B30 set: its procfs/sysfs users
also need B21-B29 (file metadata, poll/epoll, sockets, the dynamic loader). The commit therefore
claims the synthetic surface, not a corpus-program pass; the corpus claim stays with the per-batch
tickets. **OBSERVED** qtest output:

```
linux-proc: pass
linux-proc: /proc/self/status ok 294 bytes
linux-proc: /proc/self/maps ok 528 bytes
...
linux-proc: /proc/self/fd ok
linux-proc: /proc/self/exe ok 6 bytes
linux-proc: enoent ok
linux-proc: done
linuxd: hello exit=0
init exited (0)
```

## 5. Follow-ups

* **fstat/newfstatat/stat** for synthetic files and the `struct stat` copy-out (B23/B25).
* Stack growth on a fault instead of a fixed reserve; `/proc/<pid>` and the remaining procfs files
  as the process table grows (B21+).
* A corpus program that only needs procfs/sysfs once B21-B29 close its syscall set; the qtest
  fixture is the current evidence.

## 6. Provenance

First-party code; no donor source was copied. The Linux `proc(5)` field formats are public ABI
documentation; the values are deterministic RavenBSD data, not host data. `PROVENANCE.toml` is
unchanged.
