<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# pf merged-tree gate evidence

Tree: `f469b2fda2fae9ea0bffe9f74827490c8cd580fb` (merge jorge/K380), 2026-11-21.
This directory holds the small serial/log/JSON evidence for the K389 merged-tree
verification; the large images are referenced by path and SHA-256 in
`docs/pf-merged-gate.md` and `hashes.txt`.

| Acceptance case | Case name | Profile | Accel / SMP | JSON result | Duration |
|---|---|---|---|---:|---:|
| `pf-core` | pf-core | pf | TCG / 4 | pass | 97.640 s |
| `pf-kvm` | pf-kvm2 | pf | KVM / 2 | pass | 13.099 s |
| `pf-kvm4` | pf-kvm4 | pf | KVM / 4 | pass | 34.700 s |
| `pf-tcg4` | pf-tcg4 | pf | TCG / 4 | pass | 115.283 s |
| `net-postpf-pf` (`--no-build`) | net-postpf-pf | pf | TCG / 4 | pass | 27.641 s |
| `socket-inet` (`--no-build`) | socket-inet | network-config-test | TCG / 4 | pass | 288.843 s |

Each `*.json` is the harness report (`result`, `duration_ms`, `exit_status`,
matched expectations); each `*.log` is the captured serial.

## Filter-active proof (KVM)

From `pf-kvm2.log` / `pf-kvm4.log`:

```text
netd: pf loaded rules=3 nat=1
pf: nat flow ok port=34963
netd: pf block out eth0 udp 10.0.2.15:7201 -> 10.0.2.2:47002 reason=match
pf: ok
init exited (0)
```

The block line is a real packet dropped by the hook (the quick-logged UDP flow),
not a unit assertion; the NAT line shows a translated source port.

## Default profile stays unfiltered

`socket-inet` uses the non-pf `network-config-test` profile; its serial contains

```text
netd: pf absent
```

so a profile without `/etc/pf.conf` installs no filter. The dedicated
`net-postpf-default` case additionally needs a prebuilt release image
(`build/qtest/net-postpf.img`) and a root tap; that image was not present in this
clone and the case's pre-step failed before QEMU. It is not part of the K389
acceptance list; the socket-inet evidence covers the no-ruleset behaviour.

## Commands

```text
timeout 600 cargo xtask qtest --case pf-core --arch x86_64
timeout 900 cargo xtask qtest --case pf-kvm --arch x86_64
timeout 900 cargo xtask qtest --case pf-kvm4 --arch x86_64
timeout 900 cargo xtask qtest --case pf-tcg4 --arch x86_64
timeout 600 cargo xtask qtest --case net-postpf-pf --arch x86_64 --no-build
timeout 600 cargo xtask qtest --case socket-inet --arch x86_64 --no-build
```

`net-postpf-pf` and `socket-inet` were each run once without `--no-build` first
to build their case images; the acceptance-form `--no-build` runs above are the
ones recorded here.
