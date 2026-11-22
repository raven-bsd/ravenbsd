<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Workspace test-build audit — `879d3d390ba4d344928d0d3183e780645329fb4e`

Date: 2026-11-21. Ticket: K395. No test was executed; this is a compile-only audit.

| Result | Count |
|---|---:|
| Packages audited | 480 |
| Compiled | 405 |
| Failed | 75 |

Ownership: K394 `fsd-ufs` (1); K402 `blockd`/`ramfsd` `root_len` (2);
K403 `pi4-platd` host link (1); K401 host-build exclusions (71).

## Artifacts

| File | sha256 |
|---|---|
| `results.tsv` | `50a6713ff017e59675b4f721b0829a48535e618b52927f0988245af1dd8be26f` |
| `full-workspace.log` | `f19c6a863fc3ef49b6ee0263519e7d6d988f27d229082de3dae10d27e6b3c1fb` |
| `full-workspace.excerpt.log` | `e36e2dcffd369a41b3ac532a9c3a34634793649928162bdba520aaefca4c9f7c` |
| `failures/*.log` (75) | per-package compiler output |

## Commands

```text
timeout 1800 cargo test --no-run --workspace
  -> exit 101, 13.57 s; Cargo stops at the first failing compile units

timeout 1800 sh tests/release/workspace-build/run.sh
  -> 480 packages: 405 pass, 75 fail, 428.48 s
```

The full audit table, proposed owners and checkpoint recommendation are in
[`docs/releases/0.1.0-workspace-build-audit.md`](../../../docs/releases/0.1.0-workspace-build-audit.md).
The runner is [`tests/release/workspace-build/run.sh`](../run.sh).
