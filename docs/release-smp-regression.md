<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release SMP and boot-login regression

Status: K368, 2026-11-21. Evidence tags: **OBSERVED**, **PROPOSED**, **UNVERIFIED**.

This page refreshes the S107/S158 release SMP/boot-login matrix on the merged RACK master. It is
evidence-only: the boot/login path and the existing qtest cases were not edited.

## 1. Matrix on merged RACK master

* Base: `origin/master` `2e8a873f`.
* Image: `build/x86_64/ravenbsd-x86_64.img` (release profile), serial banner
  `RavenBSD kernel 0.1 (x86_64) 2e8a873f 2026-11-21`.
* Host: 20 CPUs, `/dev/kvm` present, load 21.65 -> 12.92 during the run, one other QEMU appeared
  during TCG runs 9-10. These are loaded-host samples, not an idle claim.

Command (foreground, no retry):

```sh
timeout 1200 sh scripts/smp-boot-soak.sh \
  --kvm1 10 --kvm2 10 --kvm4 10 --tcg4 10 \
  --max-load 100 --wait-for-idle 300 \
  --out build/smp-boot-soak/k368-wait
```

| Shape | Runs | Pass | p50 | p90 | worst |
|---|---:|---:|---:|---:|---:|
| KVM `-smp 1` | 10 | 10 | 3.724 s | 3.789 s | 7.223 s |
| KVM `-smp 2` | 10 | 10 | 3.668 s | 4.762 s | 6.098 s |
| KVM `-smp 4` | 10 | 10 | 3.574 s | 4.163 s | 4.625 s |
| TCG `-smp 4` | 10 | 10 | 5.894 s | 8.931 s | 11.253 s |

Every iteration has status `pass` and exit 0; none was retried. Per-iteration login times and raw
serial logs are in `tests/evidence/release-smp/` (`host.tsv`, `serial/<mode>/run-N.log`).

## 2. S107/S158 case family

The named cases also pass on the same image:

```sh
timeout 700 cargo xtask qtest --case smp-boot --arch x86_64
timeout 700 cargo xtask qtest --case release-login --arch x86_64
timeout 700 cargo xtask qtest --case login-latency --arch x86_64
```

Results: `smp-boot: pass`, `release-login: pass`, `login-latency: pass`. JSON/serial evidence is in
`tests/evidence/release-smp/qtest/`.

## 3. Forbidden lines

```sh
grep -aEi 'No child process|i/o error|ECHILD|panic|EXCEPTION|out of memory|oom-kill|killed by page fault' \
  build/smp-boot-soak/k368-wait/20261121T052228Z/*/*.log
```

0 matches. Every raw log contains a login prompt and the expected `smp: N cpus online`.

## 4. Gates

| Gate | Command | Exit | Result |
|---|---|---|---|
| KVM presence | `timeout 30 test -c /dev/kvm` | 0 | present |
| Release image | `timeout 400 cargo xtask image --arch x86_64 --profile release` | 0 | built |
| 40-boot matrix | `timeout 1200 sh scripts/smp-boot-soak.sh ...` | 0 | 40/40 pass |
| `smp-boot` | `timeout 700 cargo xtask qtest --case smp-boot --arch x86_64` | 0 | pass |
| `release-login` | `timeout 700 cargo xtask qtest --case release-login --arch x86_64` | 0 | pass |
| `login-latency` | `timeout 700 cargo xtask qtest --case login-latency --arch x86_64` | 0 | pass |
| Fast gate | `timeout 420 make check-fast` | see report | see report |

## 5. Gaps

* The matrix ran on a loaded shared host with another QEMU during the last two TCG boots; the
  login times are observed samples, not an idle <1 s boot-time claim.
* The first matrix attempt was refused by the S107 exclusive-host preflight because other
  `xtask`/`cargo` jobs were active; that attempt produced zero samples and is preserved in the
  K368 report. The recorded run waited for competing jobs before booting.
* No boot/login or qtest case was edited. A regression would be a finding for the owning lane; none
  was observed.
