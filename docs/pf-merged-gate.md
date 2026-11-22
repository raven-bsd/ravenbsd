<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# pf merged-tree release gate

Status: K389 verification, 2026-11-21. Tree
`f469b2fda2fae9ea0bffe9f74827490c8cd580fb` (K273 core, K274 pfctl, K296 KVM/NAT,
K277 conformance, K297 e2e, K380 fragment port-rule merged). Evidence:
[`tests/evidence/pf-merged/f469b2fd…/`](../tests/evidence/pf-merged/f469b2fda2fae9ea0bffe9f74827490c8cd580fb/README.md).

## 1. Gate table

| Case | Profile | Accel / SMP | Exit | Duration | Key serial | Artifact |
|---|---|---|---:|---:|---|---|
| `pf-core` | pf | TCG / 4 | 0 | 97.640 s | `pf loaded rules=3 nat=1`, block, NAT | `…/pf-core.{log,json}` |
| `pf-kvm` (case `pf-kvm2`) | pf | KVM / 2 | 0 | 13.099 s | `smp: 2 cpus online`, block, NAT | `…/pf-kvm2.{log,json}` |
| `pf-kvm4` | pf | KVM / 4 | 0 | 34.700 s | `smp: 4 cpus online`, block, NAT | `…/pf-kvm4.{log,json}` |
| `pf-tcg4` | pf | TCG / 4 | 0 | 115.283 s | `smp: 4 cpus online`, block, NAT | `…/pf-tcg4.{log,json}` |
| `net-postpf-pf` (`--no-build`) | pf | TCG / 4 | 0 | 27.641 s | `pf loaded`, block, NAT | `…/net-postpf-pf.{log,json}` |
| `socket-inet` (`--no-build`) | network-config-test | TCG / 4 | 0 | 288.843 s | `netd: pf absent`; socket/accept/echo OK | `…/socket-inet.{log,json}` |

All six acceptance cases pass; every JSON reports `result: pass`,
`exit_status: 33` (guest success) and the expected serial lines matched.

## 2. Filter-active proof

The KVM runs (and the TCG/`net-postpf` runs) contain a real dropped packet and a
translated flow, not only host tests:

```text
netd: pf loaded rules=3 nat=1
pf: nat flow ok port=34963
netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
pf: ok
init exited (0)
```

The `block out eth0 udp … -> …:47002 reason=match` record is netd's filter log
for the quick-logged test flow; the NAT port line proves the translation path.
The allowed stateful echo (`pf: allowed flow ok`) and the gateway ping round out
the allow/deny/NAT coverage.

## 3. Default profile stays unfiltered

`socket-inet` boots the non-pf `network-config-test` profile and its serial
contains `netd: pf absent` — a profile without `/etc/pf.conf` installs no filter
and the socket/accept/echo path is unaffected.

The separate `net-postpf-default` case needs a prebuilt release image
(`build/qtest/net-postpf.img`) plus a root tap; that image was absent in this
clone and the pre-step failed before QEMU. It is outside the K389 acceptance
list; the socket-inet run is the default-profile proof used here.

## 4. Tree and image build

* Tree/HEAD: `f469b2fda2fae9ea0bffe9f74827490c8cd580fb`.
* Case image used for the pf runs: `build/qtest/pf-core.img`,
  sha256 `9f92eba71bba0b3c022aa344e2763e553fbb3713a2ccc100238eea1efa542007`.
* Shared initfs: `build/x86_64/initfs.img`,
  sha256 `1b7c85c328882bf0cfcb9e962eea976910497b574674e437b7122ca679aee72d`.
* The harness builds a per-case image from the same `pf` profile; the serials
  and JSONs above are the durable evidence (small logs copied with SHA-256 in
  `hashes.txt`).

## 5. Gate commands and results

```text
timeout 600 cargo xtask qtest --case pf-core --arch x86_64          # pass, 97.640 s
timeout 900 cargo xtask qtest --case pf-kvm --arch x86_64           # pass, 13.099 s
timeout 900 cargo xtask qtest --case pf-kvm4 --arch x86_64          # pass, 34.700 s
timeout 900 cargo xtask qtest --case pf-tcg4 --arch x86_64          # pass, 115.283 s
timeout 600 cargo xtask qtest --case net-postpf-pf --arch x86_64 --no-build  # pass, 27.641 s
timeout 600 cargo xtask qtest --case socket-inet --arch x86_64 --no-build    # pass, 288.843 s
timeout 900 make check-fast                                          # see the K389 report
```

`net-postpf-pf` and `socket-inet` were first run once without `--no-build` to
create their case images (both passed); the `--no-build` forms above are the
acceptance runs.

## 6. Remaining gaps

* `net-postpf-default` (release image + root tap) was not runnable here; the
  default-unfiltered property is proven with `socket-inet`'s `netd: pf absent`.
* No new pf defect was found; verification only, no product patch in K389.
* Large images stay in `build/` and are referenced by path + SHA-256; the small
  serials/JSON are committed under `tests/evidence/pf-merged/<sha>/`.
