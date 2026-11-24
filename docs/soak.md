<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Soak/freeze harness (K228)

The release gate needs a reproducible soak that boots the release/UFS image, runs a periodic
workload, timestamps every cycle, detects a hang or freeze, and preserves the serial log plus a
machine-readable summary. K228 implements and smoke-tests that harness. It does **not** claim a
24 h pass; the owner runs the 24 h target on an idle host.

## Commands

```sh
timeout 600 make soak-smoke                  # bounded smoke, default 120 s workload
timeout 60  sh scripts/soak.sh --check       # tools/firmware + injected-stall selftest
timeout 90000 make soak-24h                  # owner-only: 86400 s workload + 3600 s margin
```

`make soak-smoke` uses `SOAK_SECONDS` (default 120), `SOAK_INTERVAL` (default 5) and
`SOAK_PROFILE` (default `release`). The 24 h target uses a 30 s interval and the same image.
The 24 h command and cap above are the owner contract; a minion session never runs it and never
reports a 24 h result.

## Workload

The script builds one case-specific image and stages a static sync fixture at `/bin/soak-sync`
and the guest workload at `/etc/soak-workload.sh`. The host sends one command:
`/bin/brush /etc/soak-workload.sh <cycles> <interval>`. Each cycle:

* `/sbin/ping -c 1 -w 2 10.0.2.2` (QEMU user-network reply);
* write, read and remove `/tmp/soak.txt` on the ramfs;
* fork/exec `/bin/uname` and remove its output;
* `/bin/soak-sync` (the product `file:` Sync call);
* `/bin/svcctl list` (the driver requires `consd running` and `vfsd running`).

Each result prints a unique marker: `soak-cycle:`, `soak-ping:`, `soak-write:`, `soak-fork:`,
`soak-sync:`, `soak-services:`, `soak-cycle-done:`, `soak-done`. The guest workload never
hard-codes a pass.

## Stall detection

The host driver timestamps each serial line and every cycle-done. The default stall threshold is
`max(30, interval * 4)` seconds, overridable with `--stall`. A missing `soak-cycle-done` within
the threshold, a nonzero workload marker, a boot/login timeout, or an unexpected QEMU exit fails
the run and preserves the logs. `--check` runs the same detector against a synthetic 2 s healthy
cadence and an injected 6 s gap with a 5 s threshold; the injected stall must be detected.

## Logs

Each run writes under `build/soak/run-<UTC>/`:

* `serial.log` — raw guest serial transcript;
* `serial.stamped.log` — host-monotonic timestamped lines;
* `soak.jsonl` — machine-readable events (cycles, markers, stall, summary);
* `summary.json` / `summary.txt` — final result, cycles, timings, image/commit and whether the
  run was a smoke or the owner 24 h run.

The case-specific image is `build/soak/image.img`; `--build` forces a fresh sync fixture,
release initfs overlay and image from the current clean checkout. The smoke uses the release
profile. A UFS run can pass a staged image with `--image`/`--profile`; this ticket's smoke
evidence is the release profile only.

## Limits and non-claims

* No 24 h pass is claimed from the smoke.
* No product, SSH, OpenSSL or Pi 4/panel changes.
* The harness does not weaken any existing gate.
