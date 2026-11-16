<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release login qtest fixture (REL2b)

This directory holds the transcript checker for the QEMU release gate. The gate case is
`tests/qtest/cases/release-login.toml`; it boots the `release` profile and drives the serial
console through login, brush, `ls /bin` and `ping 10.0.2.2`.

## Required product inputs

The fixture is ready, but the gate can only run after these land:

* **REL2a** (`profiles/release*`): the `release` profile with consd/getty/login, brush,
  virtio-net/netd/DHCP, ping, base tools and the QEMU user-network `qemu_extra`.
* **REL1** (`userland/bin/login`, `userland/bin/getty`, `/etc/passwd`, `/etc/profile`): the
  `root` / `raven` login session that execs brush.

The ticket forbids this folder from editing the profile, login or brush. Until the inputs are
merged, `cargo xtask qtest --case release-login --arch x86_64` fails at image build with
`profile release` not found; `docs/release-qemu.md` records the exact missing pieces.

## Checker

`check-transcript.sh` verifies a captured serial log:

```
sh tests/fixtures/release/check-transcript.sh build/qtest/release-login.log
```

It checks the loader/kernel banners, login and password prompts, the brush root prompt, the three
`ls /bin` entries, the first ping reply, the zero-loss ping summary and the clean init exit. The
kernel/loader lines are kept in the transcript even though the qtest only asserts a subset.

`scripts/qemu-release.sh` runs the qtest, copies the log to `build/release/qemu-release.log` and
invokes this checker.
