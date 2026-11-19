<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Shell cwd: `cd` and child processes

Date: 2026-11-19. Ticket: K160 (Otto).

## Defect

`cd bin` printed no error, but a following external `ls` still listed `/`: the shell's `pwd`
(virtual working directory) and child commands disagreed.

## Root cause

`brush_core::Shell::set_working_dir` (`userland/brush/brush-core/src/shell/fs.rs`) updates only the
shell state: it validates the target, normalizes it, sets `PWD`/`OLDPWD` and replaces the shell's
`working_dir`. It never calls `std::env::set_current_dir`, so the **process** cwd stays `/`.

External commands build a `LaunchRequest` with `working_dir` and `tokio_process` calls
`std::process::Command::current_dir`; on RavenBSD 0.1.x the native posix_spawn path does not apply
it, so the child inherited `/`. The cd builtin itself was therefore "successful" (virtual state)
while every external child ran in the old directory.

## Fix (K160 Owns)

- `userland/brush/brush-core/src/shell.rs`: new `Shell::change_dir`.
  - `absolute_path(target)` then `std::env::set_current_dir(&target)` first; a failed chdir
    returns the error **before** any shell state changes, so a failed `cd` keeps the previous
    directory.
  - then `set_working_dir(&target)` for `PWD`/`OLDPWD`/`working_dir`; if that unexpectedly fails,
    the process cwd is rolled back to the previous directory.
- `userland/brush/brush-builtins/src/cd.rs`: calls `change_dir` instead of `set_working_dir`.
- `pushd`/`popd` still use the virtual setter; they need the same `change_dir` call in a follow-up
  (outside this ticket's cd acceptance).

## Test

`tests/qtest/cases/shell-cwd.toml` (release image, login as root) covers:

1. `pwd` -> `/`.
2. `cd bin` (relative), `pwd` -> `/bin`, external `ls` lists `/bin` (the `ls` entry).
3. `cd /nonexistent` -> a `cd:` error, then `pwd` still `/bin`.
4. `cd /tmp` (absolute), `pwd` -> `/tmp`, `touch cwd-marker`, external `ls` sees `cwd-marker`.
5. `exit 0` -> `init exited (0)`.

Run:

```
timeout 120 cargo xtask qtest --case shell-cwd --arch x86_64
```

The fix also keeps the release-login path; validate:

```
timeout 120 cargo xtask qtest --case release-login --arch x86_64
timeout 240 make check-fast
```

**Validation status (closeout, 2026-11-19):** the case image built in 270 s and
`timeout 120 cargo xtask qtest --case shell-cwd --arch x86_64 --no-build` ran for 85.8 s and
failed: `pwd` moved to `/bin`, but the external `ls` child still listed `/`, so the child cwd is
dropped by the spawn/loader path. The brush-side `change_dir` is necessary but not sufficient.
Exact handoff: `libc/ravenlibc/crates/raven-libc-proc/src/native_spawn.rs` resolves the image path
with `request.cwd` but calls `loader.load(...)` with no cwd; `startup.rs` expects `AT_RAVEN_CWD`
in the auxv, which no loader/kernel code writes. Owner: Kevin/Tim (libc-proc spawn/loader) with
Stuart (kernel exec auxv). No loader/kernel path was edited. The full transcript is in the K160
report.

## Donor reconciliation (DR-52)

POSIX cwd semantics and BSD shell outcomes only: `cd` must change the process directory so
children inherit it; a failed `cd` reports and preserves the cwd. No donor code copied; the timing
of the fix is original RavenBSD code.
