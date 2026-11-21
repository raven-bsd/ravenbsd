<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K417: posix-fs-smp on aarch64 is unsupported

The case `tests/qtest/cases/posix-fs-smp.toml` declares `arch = "x86_64"` and packs the x86_64
fixture `build/fixtures/posix-fs-smp/initfs.img`. Running it with `--arch aarch64` boots the
aarch64 kernel and then rejects the init:

```text
loader: RavenBSD loader 0.1 (aarch64)
RavenBSD kernel 0.1 (aarch64) 4feacdb02 2026-11-21
smp: 1 cpus online
kernel: error: invalid init ELF: ELF machine does not match
```

- Command: `cargo xtask qtest --case posix-fs-smp --arch aarch64`
- qtest exit 1, case result `fail`, QEMU exit status 35, all expectations unmatched.
- No `smp-fs` code ran; there is no worker table, worker timeout or pending list to record.
- The aarch64 boot itself reached the kernel (clean `4feacdb02` banner) before the ELF check.

There is no aarch64 variant of this fixture in the tree (`tests/qtest/cases/posix-fs-smp*` are all
x86_64). The x86 K391 worker stall therefore cannot be compared on aarch64 with this case; a new
aarch64 fixture would be needed first.

Files: `aarch64-serial.log`, `aarch64-result.json`, `qtest-runner.log`, `meta.tsv`, `SHA256SUMS`.
