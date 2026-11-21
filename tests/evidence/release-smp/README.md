<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K368 release SMP/boot-login evidence

Merged RACK master: `2e8a873f`. Release image
`build/x86_64/ravenbsd-x86_64.img` (335,544,320 bytes), serial banner
`RavenBSD kernel 0.1 (x86_64) 2e8a873f 2026-11-21`.

## Matrix

Command (foreground, no retry):

```sh
timeout 1200 sh scripts/smp-boot-soak.sh \
  --kvm1 10 --kvm2 10 --kvm4 10 --tcg4 10 \
  --max-load 100 --wait-for-idle 300 \
  --out build/smp-boot-soak/k368-wait
```

Campaign `build/smp-boot-soak/k368-wait/20261121T052228Z/`; copied here under
`serial/<mode>/run-N.log` and `.stamped.log`.

| Shape | Runs | Pass | p50 | p90 | worst | Failures |
|---|---:|---:|---:|---:|---:|---:|
| KVM `-smp 1` | 10 | 10 | 3.724 s | 3.789 s | 7.223 s | 0 |
| KVM `-smp 2` | 10 | 10 | 3.668 s | 4.762 s | 6.098 s | 0 |
| KVM `-smp 4` | 10 | 10 | 3.574 s | 4.163 s | 4.625 s | 0 |
| TCG `-smp 4` | 10 | 10 | 5.894 s | 8.931 s | 11.253 s | 0 |

Host: 20 CPUs, `/dev/kvm` present (`crw-rw----+ root kvm 10,232`), load 21.65 -> 12.92
during the run; another QEMU appeared during TCG runs 9-10. These are loaded-host samples, not an
idle boot-time claim.

## Per-iteration login (run/login seconds; all status=pass, exit 0)

```text
KVM1  1/3.648 2/3.465 3/3.741 4/3.775 5/3.286 6/3.596 7/3.789 8/3.724 9/3.777 10/7.223
KVM2  1/6.098 2/4.762 3/3.903 4/3.648 5/3.728 6/3.563 7/3.668 8/3.385 9/3.657 10/3.855
KVM4  1/3.553 2/3.587 3/3.758 4/3.705 5/3.574 6/3.567 7/3.420 8/3.567 9/4.625 10/4.163
TCG4  1/5.894 2/7.254 3/11.253 4/8.931 5/8.127 6/6.852 7/5.832 8/4.575 9/5.120 10/4.287
```

The full per-run host/load/status rows are in `host.tsv`; the serial logs are under `serial/`.

## Case smoke

The named S107/S158 family also passed on the same image:

```text
timeout 700 cargo xtask qtest --case smp-boot --arch x86_64          -> pass
timeout 700 cargo xtask qtest --case release-login --arch x86_64     -> pass
timeout 700 cargo xtask qtest --case login-latency --arch x86_64     -> pass
```

JSON and serial logs are under `qtest/`.

## Forbidden-line sweep

```sh
grep -aEi 'No child process|i/o error|ECHILD|panic|EXCEPTION|out of memory|oom-kill|killed by page fault' \
  build/smp-boot-soak/k368-wait/20261121T052228Z/*/*.log
```

0 matches; `forbidden-grep.txt` is empty. Every raw log has a login prompt; every shape has the
expected `smp: N cpus online`.

## Artifacts

`SHA256SUMS` covers every file in this directory. The source campaign remains under
`build/smp-boot-soak/k368-wait/20261121T052228Z/` (ignored by git).
