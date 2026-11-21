<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K371 post-K273 network regression evidence

This directory preserves the K371 evidence for the default-netd and pf-profile
checks after K273 wired `raven-pf` into netd.

| Artifact | What it is |
|---|---|
| `qemu-net-check.log` | `scripts/qemu-net.sh --check`: tap/bridge prerequisites free |
| `image-build-300.log` | release image build at the ticket's 300 s cap -> **TIMEOUT 124** |
| `image-build-600.log` | same build at the measured 600 s cap -> **TIMEOUT 124** (uutils/ICU) |
| `netd-virtio-no-fileservice.log` | default netd on a profile without a file service -> **`netd: pf config: file service is not ready; refusing to start`** |
| `pf-core-first-fail.log` | first `pf-core` run: host echo/boot timing flake, probe exited before ready |
| `pf-core-retry-pass.log` | second `pf-core` run: **pass** |
| `pf-core-retry-serial.log` | serial transcript: `pf loaded rules=2`, allowed flow, blocked flow, `pf: ok`, `init exited (0)` |
| `pf-core-retry.json` | qtest result JSON (pass, 13 expectations) |
| `SHA256SUMS` | hashes of the preserved logs |

## What is proven

* The tap/bridge prerequisites are available and clean (`qemu-net: check PASS`).
* The merged pf profile loads `/etc/pf.conf` (`netd: pf loaded rules=2 nat=0`), allows
  the stateful UDP flow to 47001 (`pf: allowed flow ok`), drops and logs the 47002 flow
  (`netd: pf block out eth0 udp ... reason=match`), and exits 0.
* The first `pf-core` failure was reproduced, then a rerun passed with the same tree;
  the preserved first-failure log shows the host echo deadline, not a product verdict.

## What is not proven

* The default **release image** tap boot (DHCP + host/guest ping) is not proven: the
  release image could not be assembled inside the 300 s or 600 s cap because the uutils
  / ICU target build was still running. No current x86 release image exists in the
  workspace; older images predate K273.
* The default netd **fail-closed** path is demonstrated on `netd-virtio`, whose profile
  has no file service: `netd: pf config: file service is not ready; refusing to start`.
  The release profile inherits `shell.d/30-initfsd`, so it is expected to reach
  `netd: pf absent` instead, but that release-only distinction is unverified without the
  image.
* The ticket asks for one allowed and one denied **ICMP** flow. The staged
  `profiles/pf.d/pf.conf` contains UDP 47001/47002 rules only; K371 is forbidden to edit
  product profiles, so the ICMP variant is not claimed. The allowed/denied flow evidence
  is the staged UDP pair above.

## Reproduce

```sh
timeout 30 sh scripts/qemu-net.sh --check
timeout 300 cargo xtask qtest --case pf-core --arch x86_64
timeout 300 cargo xtask image --arch x86_64 --profile release --output build/qtest/net-postpf.img
timeout 300 env RAVEN_QEMU_IMAGE=build/qtest/net-postpf.img sh tests/scripts/qemu-net-selftest.sh
```
