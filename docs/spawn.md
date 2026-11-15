<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Runtime spawn, wait and getpid

Status: B71a host/target fixture. `spawn-smoke` exercises the `raven-rt` side; the libc
`posix_spawn` boundary is recorded below and is not changed by B71a. No kernel or hardware change is
in this ticket.

## Runtime path (OBSERVED)

`raven_rt::spawn` (`userland/rt/src/spawn.rs`) plans an `ET_EXEC`/`ET_DYN` image and its optional
`PT_INTERP` interpreter, creates the child with `PROC_CREATE`, maps the segments and a 256 KiB
stack (or a 64 KiB stack when the `alloc` feature is off), builds the PG-50 auxv and starts the
first thread. The caller receives the child's process handle.

The stack mapping must start at `STACK_TOP - stack_size` and use the same `stack_size` for the
memory object, the parent scratch mapping and the child mapping. The 64 KiB no-`alloc` build
previously created a 256 KiB memory object but mapped only 64 KiB at `STACK_TOP - 256 KiB`, so
`sp0` was outside the child stack and `THREAD_CREATE` returned `EFAULT`; B71a maps the buffer at
`STACK_TOP - stack_size` and `spawn-smoke` covers that path.

`raven_rt::spawn::RtSys::proc_wait(handle, &mut code)` calls `PROC_WAIT` and stores the exit code.
The parent's own pid comes from `PROC_SELF_INFO` (syscall `0x35`, eight `u64` words; the first word
is the pid). `spawn-smoke` exercises exactly that sequence:

```text
spawn-smoke: start
spawn-smoke: ok pid=<pid> exit=7
init exited (0)
```

Run it with:

```
cargo xtask qtest --case spawn-smoke --arch x86_64
```

The fixture is `tests/fixtures/spawn`; it packs `/init` and `/child` into the kernel-mapped initfs,
so it does not depend on the libc test harness or on PG-22/PG-23. The child is a `raven-rt` binary
that prints one line and exits 7; the parent reads the same initfs with `map_initfs()` and
`Initfs::parse`, which is the contract `spawn_with` expects for the `Handle::INITFS` mapping.

## The libc boundary (OBSERVED, outside B71a's Owns)

The target `libc-spawn` failure is not in `raven-rt`:

* `libc/ravenlibc/crates/raven-libc/src/process/posix_spawn.rs` and `waitpid.rs` call
  `crate::ffi::procbackend::backend()`.
* `libc/ravenlibc/crates/raven-libc/src/ffi/procbackend.rs:32-34` returns
  `SyscallBackend::new(RealSyscall)` on the target.
* `libc/ravenlibc/crates/raven-libc-proc/src/syscall_backend.rs:96-103` constructs the backend
  with `local: LocalBackend::new(None)`.
* `syscall_backend.rs:170-184` delegates `spawn`, `fork`, `exec` and `wait` to that local backend.
* `local_backend.rs:76-90` returns `ENOSYS` for `spawn`/`fork`/`exec` and `ECHILD` for `wait` when
  no early spawner is installed.

Therefore `posix_spawn` returns `ENOSYS`, `waitpid` returns `ECHILD`, and the `getpid` check in
`libc-spawn.c` also fails because it spawns a child first. `getpid` itself uses `PROC_SELF_INFO`
and is not the fault.

The fix belongs to the libc/procd boundary: either install an `EarlySpawn` implementation (the
`raven-rt` spawn path) into `LocalBackend` from the shell, or land PG-44 `procd` and route the
backend through it. Follow-up: the libc owner/kevin (C64/C74 lane) or the PG-44 ticket; B71a does
not edit `libc/**` or `tests/libc/**`.

## Evidence tags

* OBSERVED: the runtime API and the `posix_spawn` call path above, read from the current tree.
* OBSERVED: `cargo test -p raven-rt` 113 passed and the `spawn-smoke` qtest result in the B71a
  report.
* UNVERIFIED: the libc fix and the PG-44 procd integration; both are follow-ups.
