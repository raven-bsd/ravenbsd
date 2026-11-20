<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Boot presentation

K191 makes the release console readable: the login prompt is the last line, `/root` exists in
the initfs, and the per-service timing diagnostics are opt-in.

## Login last

`svcd` starts every other enabled service first and then starts `getty`. Its worker runs from the
root filesystem and its banner/`login:` is the final console output. A getty failure is still
printed and still fails the boot; it is not hidden.

The service lifecycle lines (`svcd: <name> up`) and a concise report remain:

```
svcd: ... up
svcd: all services up
RavenBSD 0.1 (x86_64) (tty:console)
login:
```

When any enabled service is not up, the report is `svcd: boot report: <up> up, <failed> failed`
and each failure keeps its `svcd: <name> failed: <reason>` line.

## `/root`

`profiles/shell.d/00-root.toml` stages `/root/.keep` in the shell initfs. The release profile
inherits it, so `HOME=/root` and `cd` work without a UFS root. The Pi 4/UFS payloads keep their
own `/root/.keep` in the root filesystem.

## Timing diagnostics

The per-service `svcd: <name> timing: ...` lines are **off** by default in the release profile:

* `profiles/release.d/05-boot-quiet.toml` stages `/etc/raven/boot-quiet`.
* To enable the timing diagnostics, stage `/etc/raven/boot-verbose` (content `1`) in the initfs,
  or place it in a writable `/etc/raven` on a UFS root before boot. A non-zero file wins over the
  quiet marker.
* Images without `/etc/raven/boot-quiet` (the shell/test diagnostic transcripts such as
  `shell-cwd`) keep the timing lines unless their profile adds the quiet marker.

The `svcd: <name> failed: ...` and `crash loop` lines are always printed; only the timing
diagnostics are gated.

## Evidence

`release-login` boots without any input and waits for `login:`; it then checks `cd && pwd` is
`/root` and the normal login/ping transcript. The 2026-11-20 K191 runs use the release image built
from the ticket branch; a verbose image is built by injecting `/etc/raven/boot-verbose` into the
release initfs and booting it with `release-login --no-build` to verify the timing lines remain.
No Pi 4 board result is claimed here.
