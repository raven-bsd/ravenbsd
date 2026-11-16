<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# pi4-bringup fixtures

Host-side fixtures for `scripts/pi4-bringup.sh`. No board transcript is included: the real
evidence must come from the owner's REL3 capture.

| File | Purpose |
|---|---|
| `expected-sequence.txt` | Ordered marker table (`stage|marker|meaning`) printed by `--markers` and mirrored in `docs/pi4-bringup.md`. The REL3 gate file remains authoritative. |
| `hash-fixture.txt` | A small non-bootable file used only to exercise the `--check -i ... --sha256 ...` hash path on a host without the release image. |
| `hash-fixture.sha256` | The SHA-256 of `hash-fixture.txt`. |

Never flash `hash-fixture.txt`; it is not an image. A synthetic transcript would not substitute
for a board log, so none is provided.
