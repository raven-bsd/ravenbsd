# uutils base-tools evidence (K25a staging)

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

The vendored uutils workspace is under `userland/uutils` and the build command is
`cargo xtask build uutils --arch x86_64`. The target build links the 30 selected utilities for
`x86_64-unknown-ravenbsd`; the host test slice passes 97 tests.

The image switch is three files:

1. `profiles/shell.d/72-uutils.toml` (retires the ten `[[initfs.programs]]` entries in
   `profiles/shell.d/70-file-tools.toml`; the first image carries the base-tools subset while the
   initfs budget is settled):

   ```toml
   [[initfs.files]]
   src = "build/x86_64/uutils/ls"
   path = "/bin/ls"
   mode = 0o755
   generated = true

   # ... one block per selected name; `cargo xtask build uutils --arch x86_64` writes them.
   ```

2. the profile `shell-test` (its `shell.d` fragments include `72-uutils.toml`).
3. `tests/qtest/cases/base-tools.toml`, registered from `tests/uutils/base-tools.toml`, run with
   `cargo xtask qtest --case base-tools --arch x86_64`.

The enlarged ESP (220 MiB in a 320 MiB image) fits the std toolset. On the last boot the kernel
reached userspace but `initfsd` did not report ready within its 60 s u16 timeout; the readiness /
initfs budget is the open qtest item. The shared `ravenbsd` toolchain was removed from the
workspace before the trimmed-image rerun, so the case could not be repeated in that session.
