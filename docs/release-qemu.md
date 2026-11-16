<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU release login gate (REL2b)

Status: case, fixture checker, transcript script and this document are ready; the product inputs
from REL1/REL2a are still in flight, so the gate is **PARTIAL**. No board claim.

## Gate

```
cargo xtask qtest --case release-login --arch x86_64
```

The case boots the `release` profile and drives the serial console:

1. wait for the login prompt and send `root`;
2. wait for the password prompt and send `raven`;
3. assert the brush root prompt (`user@host`, cwd and `#`);
4. send `ls /bin` and assert `cat`, `ls` and `uname`;
5. send `ping 10.0.2.2` and assert a first reply plus the zero-loss summary;
6. send `exit 0` and assert `init exited (0)`.

QEMU user-mode networking supplies the guest lease and the `10.0.2.2` gateway; the case adds the
virtio-net device. The full serial capture keeps the loader and kernel banners even though the
case asserts only their presence.

## Transcript

`make release-gate` runs the full host check and the qtest. The standalone wrapper:

```
sh scripts/qemu-release.sh
```

runs the qtest, copies `build/qtest/release-login.log` to `build/release/qemu-release.log` and
checks the required lines with `tests/fixtures/release/check-transcript.sh`.

Host tests for the harness package:

```
cargo nextest run -p raven-qtest
```

## Required product inputs

The qtest cannot reach the login prompt until these are merged:

* **REL2a** — `profiles/release*` with getty/login, brush, virtio-net/netd/DHCP, ping, base tools
  and the release service list.
* **REL1** — `userland/bin/login`, `userland/bin/getty`, `/etc/passwd`/`/etc/profile` and the
  root/raven session that execs brush.

REL2b owns only the qtest case, release fixture, `scripts/qemu-release.sh` and this document. It
must not edit the profile, login or brush. Until the inputs land, the image build reports that
the `release` profile is missing; the case and checker stay ready for the merge.

## Evidence

OBSERVED on `stuart/REL2b` (base `origin/master` `52864a6c`): the case parses, the fixture checker
is syntactically valid, and `cargo xtask qtest --case release-login` fails at image build because
`profiles/release.toml` does not exist. See `tickets/stuart/reports/REL2b.md` for the exact
command output.

## Limits

* No board claim; the Pi 4 release gate is REL3/REL4.
* The default `root` / `raven` credential is a development default; change it before exposing the
  VM or a board to a network.
* The current `shell-test`/`shell-session` profile uses `rsh`, not login/brush, and is not a
  substitute for this gate.
