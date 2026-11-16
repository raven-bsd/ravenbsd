<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# std `posix_spawn` on x86_64-unknown-ravenbsd (K91)

Status: K91 implementation. The Rust toolchain patch series now selects std's `posix_spawn`
implementation for `x86_64-unknown-ravenbsd` and the patched target libc declares the
`posix_spawn*` ABI it calls. A clean `make toolchain` no longer regresses to the `fork` path,
which returns `ENOSYS` on RavenBSD.

## Why

`std` on the RavenBSD target used the `fork`/`exec` process path because the target was not in
the `posix_spawn` cfg lists, and the target libc did not declare `posix_spawn`. Brush and other
std consumers therefore hit the missing `fork` path. The target libc declarations were committed
to `toolchain/vendor/libc`, but the generated patch series did not contain them, so a clean
toolchain rebuild would lose them.

K91 adds one patch after the existing target and libc patches:

| Patch | Contents |
|---|---|
| `0001-add-ravenbsd-targets.patch` | target specs, `Os::Ravenbsd`, supported-target registration, std cfg changes |
| `0002-libc-ravenbsd.patch` | vendored libc 0.2.185 with the `ravenbsd` unix module |
| `0003-posix-spawn-ravenbsd.patch` | std `posix_spawn` cfg + target-libc `posix_spawn*` ABI (K91) |

## What 0003 changes

`library/std/src/sys/process/unix/unix.rs`:

* `target_os = "ravenbsd"` in the `posix_spawn` fallback exclusion list and in the real
  `posix_spawn` implementation list, so std compiles the spawner instead of the `Ok(None)`
  fallback.
* `target_os = "ravenbsd"` in both `get_posix_spawn_addchdir` cfg lists. The first function uses
  `weak!` lookup, which does not work in a static binary; RavenBSD takes the compile-time branch
  and links `posix_spawn_file_actions_addchdir_np` directly.

`library/libc-ravenbsd/src/unix/ravenbsd/mod.rs`:

* `use crate::pid_t;`
* `posix_spawnattr_t` (336-byte opaque data, 8-byte aligned) and
  `posix_spawn_file_actions_t` (80-byte opaque data, 8-byte aligned).
* `POSIX_SPAWN_RESETIDS`, `SETPGROUP`, `SETSIGDEF`, `SETSIGMASK`, `SETSCHEDPARAM`,
  `SETSCHEDULER` and `SETSID`.
* The `posix_spawn`, `posix_spawnp`, file-action init/destroy/adddup2/addclose/addchdir_np and
  attr init/destroy/setpgroup/setsigdefault/setflags externs.

The declarations mirror `toolchain/vendor/libc/src/unix/ravenbsd/mod.rs`; that file remains the
reviewable source copy of the libc delta. The opaque sizes and constants are the ABI contract
between the target libc and the RavenBSD runtime.

## Patch generation

The patch is generated deterministically from a pristine Rust 1.97.1 source tree after 0001 and
0002 have been applied:

1. Apply the std cfg edits and the libc declarations to the two files.
2. Generate each diff with `diff -u --label a/<path> --label b/<path> <base> <target>`.
3. Concatenate the diffs under the SPDX header.

The patch contains no timestamps, host paths or temporary directory names. It applies with
`patch -p1` after 0001/0002; `patch --dry-run` was used before the build. SHA-256:

```
c734f6bb2e05ed5137231300c54063979456cf5bed5f0d3f98245bcb47192e13  0003-posix-spawn-ravenbsd.patch
```

`toolchain/build.sh` includes every patch hash in its source and installed-prefix fingerprints.
Adding 0003 therefore forces one source re-extract and rebuild; later runs with the same series
reuse the stamps.

## Build and acceptance

The K91 acceptance runs, in order:

```sh
make toolchain
rustc +ravenbsd --print target-list | grep ravenbsd
cargo xtask libc-pack --arch x86_64
cargo xtask build brush --arch x86_64
cargo xtask qtest --case release-login --arch x86_64
make check-fast
```

`make toolchain` must apply 0001/0002/0003, install the stage-1 compiler under `build/toolchain`,
and link it as the `ravenbsd` rustup toolchain. The rebuilt target std is under
`build/toolchain/lib/rustlib/x86_64-unknown-ravenbsd/lib`. The exact patch hash, build result and
std path are recorded in `tickets/kevin/reports/K91.md`. **OBSERVED** there.

## Runtime proof and limits

* K91 proves the compiler/std build, the target list and the libc/brush rebuilds. The
  `brush-spawn` qtest that exercises an actual spawned child belongs to P9c; it is the final
  runtime proof, not a K91 claim.
* The cfg change applies to every `*-ravenbsd` target, but only `x86_64-unknown-ravenbsd` is
  built and exercised here. The aarch64 and riscv64 target specs remain draft.
* `posix_spawn` still requires the ralibc implementation behind the symbols; K91 declares the
  ABI and selects the std path, it does not implement the runtime spawn itself.
* No nightly compiler, no `RUSTC_BOOTSTRAP` and no new dependency is used. The source is the
  pinned Rust 1.97.1 tarball with the reviewed patch series.
