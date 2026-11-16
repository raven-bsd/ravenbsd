<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `tests/spawn/` fixture (P9 planning)

This directory is the P9 test-planning fixture. It is not wired into `cargo xtask qtest` yet;
P13 moves the case files to `tests/qtest/cases/` when the implementation exists.

## Files

* `expected-sequence.txt` — the exact ordered `Sys` calls and arguments that the extracted
  loader must produce for a two-segment image with an empty fdmap. It is the contract the P10
  host mock asserts, complementing the existing `userland/rt/src/spawn_ut.rs` cases.
* `cases/brush-ls.toml` — release-image qtest sketch for `/bin/brush -c '/bin/ls /bin'`.
* `cases/brush-ping.toml` — release-image qtest sketch for
  `/bin/brush -c 'ping -c 1 10.0.2.2'`.

## Host fixture

When `crates/raven-spawn` lands (P10), its `*_ut.rs` reads `expected-sequence.txt` as a table of
expected calls and replays the same fake image through `spawn_image_with` with a recording
`Sys`. The fixture is intentionally textual so a reviewer can diff the syscall contract without
building a target.

The negative assertions matter as much as the calls:

* no `HANDLE_TRANSFER` in the console-only path;
* no `PROC_SETPGID`, `PROC_SETSID` or `fork`/`exec`;
* `PROC_HANDLE_INFO` is called exactly once per spawn;
* the returned thread handle is closed (the current loader ignores it, which leaks one handle
  per spawn).

## Target evidence

The follow-up reports the qtest exit codes and guest lines. P9 runs no target command and makes
no QEMU or board claim.
