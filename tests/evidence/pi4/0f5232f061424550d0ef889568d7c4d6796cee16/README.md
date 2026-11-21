<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K383 Pi 4 readiness evidence

Host-side outputs for tree `0f5232f061424550d0ef889568d7c4d6796cee16`:

| Artifact | What it is |
|---|---|
| `pi4-release-check.log` | `scripts/pi4-release.sh --check` (manifest/kernel/initfs/FDT) |
| `pi4-ui-check.log` | `scripts/pi4-ui-evidence.sh --check` (SD layout/providers; board blocked) |
| `board-pi4-check-dryrun.log` | `scripts/board-pi4-check.sh --dry-run` marker checklist only |
| `image-hashes.txt` | SHA-256 of the four staged artifacts |
| `image-layout.txt` | sizes, mtimes, GPT layout and file(1) output |
| `SHA256SUMS` | hashes of these logs |

The three Pi 4 gates (SD/UFS serial boot, HDMI/USB login, E-Ink photo) remain **HARDWARE** and are
not claimed by these logs. The board check refuses QEMU/virt/synthetic logs by design.
