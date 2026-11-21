<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K384 freeze-checklist verification (base `0f5232f0`)

Cheap checks for `docs/releases/0.1.0-freeze-checklist.md` at
`0f5232f061424550d0ef889568d7c4d6796cee16`. No QEMU, image, soak or check-fast was run.

## Files

| File | Content |
|---|---|
| `sh-n-doc.log` | literal `sh -n` on the Markdown checklist: **exit 2** — Markdown is not shell; the ticket's acceptance line needs a file fix |
| `runbook-commands.sh` | extracted runbook shell commands; parsed with `sh -n` |
| `sh-n-runbook-commands.log` | `sh -n runbook-commands.sh` exit 0 |
| `check-paths.sh`, `paths.txt` | 31 cited paths present, 2 missing: `scripts/pi4-eink-evidence.sh` (E1/E3) and `tests/qtest/cases/hdmi-console.toml` (HDMI1) |
| `check-sh-n.sh`, `sh-n-scripts.log` | 13/13 cited shell scripts parse cleanly |
| `xtask-help.log` | `cargo xtask --help` exit 0 |
| `docs.log` | `make docs` exit 0 |
| `evidence.log`, `evidence-check.log` | 18/19 complete (K253 only); `evidence check: PASSED` |
| `board.sha256` | `tickets/release/0.1.0-gates.tsv` hash used for the reconciliation |

## Notes

The checklist's K384 section is the current disposition; the detailed tables below it retain
historical commit stamps. Heavy gates are QUIET-CHECKPOINT, not executed here. The two missing
commands are owner/board deliverables (E-Ink E1/E3 and HDMI1), not K384 code paths.
