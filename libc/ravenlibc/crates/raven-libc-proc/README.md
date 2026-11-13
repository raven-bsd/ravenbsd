<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-proc

Pure process logic for ravenlibc. The kernel has no `fork`, `exec`, pids or signals on Raven,
so the C process API is a thin client of the `procd` userspace server; this crate holds the
parts that are pure logic and host-testable.

The crate is `no_std` plus `alloc`, class P (`#![forbid(unsafe_code)]`), layer L1, and depends
only on `raven-libc-abi`. It has no statics, locks, clocks or environment reads: every effect
enters through a trait or an argument.

## Contents

* `exec_path` - the `execvpe` `PATH` search, generic over a try-exec closure.
* `spawnp` - the `posix_spawnp` empty/slash/search prechecks.
* `spawn_model` - the `posix_spawn` attribute and file-action API, the scratch fd-table
  applier, the attribute order and the driver.
* `wait_status`, `wait`, `waitpid`, `wait3`, `waitid` - the Linux status codec and the
  `wait4`/`wait6` mappings.
* `times`, `nice`, `popen_model`, `daemon`, `initgroups`, `getprogname`, `setprogname` - the
  small transpiles.
* `atfork`, `lock_registry` - the atfork registry and the cooperative fork-lock registry.
* `rlimit_model`, `ids` - the 16-resource table and the `SETCRED` builders.
* `startup`, `auxv`, `getauxval`, `procinfo` - the C8 stack parser, auxv table, `getauxval` and
  the procinfo seqlock reader.
* `wire` - the proposed `procd` protocol encoder and response decoder.
* `backend`, `local_backend`, `fake` - the `ProcBackend` seam, the pre-procd backend and a
  deterministic fake.

## Checks

```
cargo test -p raven-libc-proc
cargo clippy -p raven-libc-proc --all-targets -- -D warnings
cargo build -p raven-libc-proc --target x86_64-unknown-none
cargo doc -p raven-libc-proc --no-deps
cargo run -q -p raven-provenance -- check --root .
cargo xtask libc-lint
```

`tests/ref_fdtable.rs` is the independent reference interpreter for the scratch fd-table
applier; it compares 2000 seeded action lists.
