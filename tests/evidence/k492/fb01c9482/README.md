<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K492 boot-loop evidence

Product tree: `240d92d70bfac9f9242ddaa2ae1f1beed9016c77` (origin/master).
Harness commit: `fb01c9482e7dd24184ecd5daa208f6fc2fb0134f` (tests/soak/run.py + run.sh).
Image: `build/x86_64/ravenbsd-x86_64.img`,
sha256 `0fa5f6554d5fdd88f81f27de41bedb69fae148d9f91d59ba3148640e592e4806`.
The image was built from the clean product tree before the harness commit; the
harness is not part of the image.

## Matrix

60/60 boots reached `login:` (the exact getty line is `login: ` with a trailing
space; the tables store it stripped as `login:`). Every boot also printed the
mode's `smp: N cpus online` line. No serial contains `panic`, `EXCEPTION`,
`svcd: ... crash loop`, `restart`, `unclaimed vector`, `CPU Reset` or
`triple fault`, checked independently with a case-insensitive grep over every
raw and stamped serial.

| Mode | Boots | Pass | login_s min | login_s max | Heavy boots |
|---|---:|---:|---:|---:|---|
| tcg4 | 20 | 20 | 8.785 | 53.449 | 16-20 |
| kvm2 | 20 | 20 | 5.326 | 17.518 | 16-20 |
| kvm4 | 20 | 20 | 7.148 | 20.120 | 16-20 |

`boots.tsv` is the full 60-row table: mode, boot, status, exact prompt line,
login seconds, wall seconds, load1 before/after, other QEMU/job counts, the
heavy marker and the serial path. The per-mode `summary.tsv`, `summary.json`,
raw and stamped serials, and the runner's own `SHA256SUMS` are under
`tcg/`, `kvm2/` and `kvm4/`.

## Host load

The host is shared with other lanes. During the matrix, `/proc/loadavg` was
32-95 on 20 CPUs and `ps` counted 2-6 other QEMU plus 12-42 other
xtask/cargo/rustc/clippy jobs. The ticket's "second heavy job" was eight
`yes(1)` CPU burners started before boot 16 and stopped after boot 20 in each
mode; their load1 ranges are in `boots.tsv` (tcg 52.45-73.79, kvm2
55.57-64.62, kvm4 59.54-81.19). The baseline boots are therefore
"ambient-load" boots, not an idle-host baseline; the load is stated rather
than hidden.

## Attempt 1 timeout (kept, not hidden)

The first TCG attempt used a 45 s per-boot cap and stopped at boot 4:
`login-failed reason=timeout`. The serial (`attempt-1-timeout-45s/serials/
tcg-boot-04.log`) shows the boot still progressing at `svcd: devd up` with
load1 67.14-76.57; there is no panic, EXCEPTION, crash loop or restart. The
cap was too tight for the measured slowdown, so the reported matrix uses a
180 s TCG / 120 s KVM per-boot cap (5.4x / 6.9x the slowest measured boot) and
was rerun from boot 1. Both runs are preserved.

## Commands

```text
timeout 600  cargo xtask image --arch x86_64 --profile release
timeout 3600 sh tests/soak/run.sh --mode tcg  --boots 20 --load-processes 8 --timeout-tcg 180
timeout 3600 sh tests/soak/run.sh --mode kvm2 --boots 20 --load-processes 8 --timeout-kvm 120
timeout 3600 sh tests/soak/run.sh --mode kvm4 --boots 20 --load-processes 8 --timeout-kvm 120
timeout 900  make check-fast
```

`logs/` holds the three final host transcripts, the attempt-1 transcript, the
selftest mutation, and the first check-fast cap expiry.
