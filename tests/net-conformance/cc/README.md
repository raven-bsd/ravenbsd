# K289 CUBIC/RACK packet-level scenarios

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

Test-only bulk sender and runner for the CUBIC/RACK wire scenarios in
`docs/tcp-cc-conformance.md`.

* `bulk.c` — `/bin/tcpcc`, a 256 KiB TCP sender on port 23091.
* `tcpcc.toml` / `svcd.conf` — the one-shot initfs service overlay.
* `build.sh` — builds the sender with the RavenBSD libc sysroot and packs
  `build/fixtures/cc/image.img`.
* `run.sh` / `run.py` — tap/namespace plumbing and the four raw-peer scenarios:
  `slow-start`, `congestion-avoidance`, `loss-recovery`, `reordering`.

No product source, profile or contract file is edited. The runner reuses the
K253 `harness.py` raw peer and frame builders.
