<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 4 GiB console-input isolation (K164)

S157 booted the 4 GiB/4 vCPU release image, logged in, echoed `uname -a` correctly, but the
brush shell received `error: command not found: \0\0\0\0\0\0\0\0`. This harness isolates whether
the NULs come from the serial/qtest write path, the console/tty read path, or the shell line
editor.

## Harness

```
sh scripts/console-input-4g.sh --size 512M
sh scripts/console-input-4g.sh --size 4G
sh scripts/console-input-4g.sh --size 4G --shell rsh
sh scripts/console-input-4g.sh --size 4G --spacing 1000
sh scripts/console-input-4g.sh --size 4G --accel kvm
```

`tests/fixtures/console-input-4g/measure.py` boots the existing S157 `smp-4g` image read-only
(`-snapshot`), logs in, and sends `echo hi`, `pwd` and `uname -a` with a controlled 100 ms or 1 s
spacing. It captures the raw serial bytes (binary `serial.log`), a per-response hexdump
(`responses.hex`), a table and a JSON result. `--extra-command` adds a controlled line without an
expected marker (NULs or `command not found` still fail the sample). The default acceleration is
TCG, matching S157; `--accel kvm` is a supplemental control.

The sample is `clean` only when every response has zero NUL bytes, no `command not found`, and
the expected marker (`hi`, `/`, `RavenBSD raven 0.1 ... x86_64`). The script exits 0 for a clean
sample, 1 for a reproducible NUL/failure, and 2 for a harness/host error.

## What to compare

* **512M vs 4G** at the same shell and spacing.
* **brush vs rsh** (the release login default is brush; `rsh` is started from the brush prompt).
* **100 ms vs 1 s** spacing, to rule out a send/read race.
* **TCG vs KVM**, to separate a TCG memory-layout artifact from a guest console bug.
* The preserved S157 serial log is the NUL reference; a clean run does not overwrite it.

A loaded-host sample is recorded as loaded, not as a pass. The raw serial log is the evidence;
the hexdump makes the NUL count and the echoed line visible side by side.

## Measured on the S157 image (`git:7ee0dbcd`, sha256 `03cbe70a…`)

| size | shell | spacing | accel | `echo hi` | `pwd` | `uname -a` |
|---|---|---|---|---|---|---|
| 512M | brush | 100 ms | TCG | clean | clean | `command not found: uname`, 0 NULs |
| 4G | brush | 100 ms | KVM | clean | clean | `command not found: uname`, 0 NULs |
| 4G | brush | 100 ms | TCG | clean | clean | `command not found: uname`, 0 NULs |
| 4G | rsh | 100 ms | TCG | clean (rsh started) | clean | `command not found: uname`, 0 NULs |
| 4G | brush | 1 s | TCG | clean | clean | `command not found: uname`, 0 NULs |
| 4G qtest | brush | 100 ms | TCG | clean | clean | `command not found: uname`, 0 NULs |

The S157 NULs **did not reproduce** in these samples. `/bin` lists `uname`, but both `uname` and
`/bin/uname` fail to spawn (`command not found`), so this old image cannot meet the "clean
`uname -a`" acceptance even though its line input is clean. The 1 s spacing and the rsh control
rule out the simple qtest/serial-write and brush-editor hypotheses for these samples.

## Handoff

* **Mark** (memory/console ring): the only NUL evidence remains the S157 preserved
  `build/s157/smp-4g-rev253-uname-nuls.log`; it is intermittent and did not reproduce here.
* **Phil** (brush/tty): `echo hi`/`pwd` are clean in both brush and rsh at 4G, so the line
  editor is not corrupting ordinary input in these samples.
* **Stuart / S157 owner**: the `7ee0dbcd` image's `/bin/uname` is unspawnable at both 512M and
  4G; a fresh base image is needed to judge the acceptance `uname -a` line.

No donor code is involved; this is console/terminal behaviour only.
