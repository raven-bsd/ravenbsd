# uutils base-tools evidence (K25a staging)

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

The vendored uutils workspace is under `userland/uutils` and the build command is
`cargo xtask build uutils --arch x86_64`. The host test slice passes; the Raven target build is
blocked on the external `libc`/`getrandom` crates (see the K25a report).

When the target build is unblocked, the image switch is three files:

1. `profiles/shell.d/72-uutils.toml` (replaces the ten `[[initfs.programs]]` entries in
   `profiles/shell.d/70-file-tools.toml`; the old entries stay until then):

   ```toml
   [[initfs.files]]
   src = "build/x86_64/uutils/ls"
   path = "/bin/ls"
   mode = 0o755
   generated = true

   # ... one block per selected name; `cargo xtask build uutils --arch x86_64` writes them.
   ```

2. `profiles/base-tools-test.toml` (extends `shell`) plus its `shell.d` fragment.
3. `tests/qtest/cases/base-tools.toml` copied from `tests/uutils/base-tools.toml` and run with
   `cargo xtask qtest --case base-tools --arch x86_64`.

The draft case is not registered in `tests/qtest/cases` yet: registering a case whose profile
cannot build the binaries would turn the image build red without testing anything. The case file
here is the reviewed script for the next round.
