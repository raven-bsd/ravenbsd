<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# credentials (S179/S187) — aarch64 divergence

`tests/conformance/credentials/` is a self-contained x86-64 row, not a shared qtest table, and it
cannot run unchanged on aarch64:

* `tests/conformance/credentials/build.sh:17` `arch=x86_64`; `:37` `console=ttyS0`.
* `tests/conformance/credentials/probe/build.sh:16` `arch=x86_64`; `:44`/`:55`
  `--target=x86_64-unknown-ravenbsd`; `:56` `tests/libc/target/x86_64.ld`.
* `tests/conformance/credentials/run.py:412` `qemu-system-x86_64`/`-machine q35`; `:45-46`
  OVMF firmware.

Owner: steve (S179/S187). Resuming experiment: build the same probe with the aarch64 sysroot
(`--target=aarch64-unknown-ravenbsd`, `tests/libc/target/aarch64.ld`), build the aarch64 release
image with `console=ttyAMA0`, boot AAVMF/virt with the same sshd user networking, and reuse
`check_rows` from the S179 runner. The S182 runner detects this row, reports this divergence and
exits 1; it never silently skips it.
