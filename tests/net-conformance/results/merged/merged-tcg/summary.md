<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K275 matrix: merged-tcg

* commit: `0a641914`
* image: `build/qtest/net-conformance.img`
* image sha256: `c428e598739e5df7bbee55b9a257206ed7837c6eaf04d7dda64f409b223d2a19`

| Scenario | Result | Detail | pcap | serial |
|---|---|---|---|---|
| `handshake` | pass | SYN-ACK mss=1460 ws=3 sack=True; plain-peer ws=absent win=65535 | `tests/net-conformance/results/merged/merged-tcg/handshake.pcap` | `tests/net-conformance/results/merged/merged-tcg/handshake-serial.log` |
| `zero-window` | pass | probe=1B seq=1; after-reopen=43B; total=44/44; frames=1; replayed-probe=no | `tests/net-conformance/results/merged/merged-tcg/zero-window.pcap` | `tests/net-conformance/results/merged/merged-tcg/zero-window-serial.log` |
| `window-update` | pass | delivered=44/44 frames=1 | `tests/net-conformance/results/merged/merged-tcg/window-update.pcap` | `tests/net-conformance/results/merged/merged-tcg/window-update-serial.log` |
| `retransmit` | pass | first=44B seq+0; retransmit=yes | `tests/net-conformance/results/merged/merged-tcg/retransmit.pcap` | `tests/net-conformance/results/merged/merged-tcg/retransmit-serial.log` |
| `fin-rst` | pass | delivered=44/44; peer-close=FIN | `tests/net-conformance/results/merged/merged-tcg/fin-rst.pcap` | `tests/net-conformance/results/merged/merged-tcg/fin-rst-serial.log` |
