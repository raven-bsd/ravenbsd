<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 release network evidence fixture

This directory holds the **host-model** transcript and a negative fixture for the REL4 evidence
script. Neither file is Pi 4 board evidence:

* `model-transcript.log` is the stable transcript shape produced by the host-tested
  `raven_genet::release::ReleaseGate`. The `GENET-NET-*` lines are synthetic; the surrounding
  `netd`/`ping` lines show the expected console context.
* `model-missing-ping.log` is the same transcript without the ping and completion markers. The
  script must reject it.

Run the pair without QEMU or a board:

```
scripts/pi4-net-evidence.sh --self-test
```

The board owner replaces the synthetic markers with the serial lines from the owner-run Pi 4
release image and keeps the `netd: interface eth0 ...` plus `ping: ...` lines. A file with
placeholders or without every marker fails the script; see `docs/pi4-net.md`.
