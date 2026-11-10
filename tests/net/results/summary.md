<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# packetdrill harness summary

Topology: SUT `10.0.0.1:8080` (`02:00:00:00:00:01`), peer `10.0.0.2:12345` (`02:00:00:00:00:02`), peer ISN 1000, in-memory `PairDevice`.

| Script | Result | Duration (ms) | Notes |
|---|---|---|---|
| `data-ack.pkt` | pass | 12 | ok |
| `fin-close.pkt` | pass | 0 | ok |
| `reset.pkt` | pass | 0 | ok |
| `retransmission.pkt` | pass | 12 | ok |
| `syn-handshake.pkt` | pass | 0 | ok |
| `window-update.pkt` | pass | 0 | ok |

Totals: 6 script(s), 6 pass, 0 known-fail, 0 fail.
