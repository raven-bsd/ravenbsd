<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# UFS release gate — regression record

Current verdict (2026-11-22): tree `722d3109`, branch `mel/K413`, QEMU 6.2.0 on
x86_64. Evidence: `tests/evidence/ufs/722d3109/`.

## Gates

| Gate | Result | `duration_ms` |
|---|---|---:|
| `ufs-disk` (first boot, write, disk tools) | PASS | 327149 |
| `ufs-disk-persist` (second boot, read marker) | PASS | 58007 |
| `ufs-disk-ufs1` (UFS1 mount/write/read) | PASS | 80527 |
| `ufs-root` (UFS2 root pivot, rw `/`, marker) | PASS | 180051 |
| `ufs-root-persist` (same root disk, read marker) | PASS | 120013 |
| `ufs-logout` (exit returns to `login:`) | PASS | 90203 |
| `disk-tools` (scratch GPT, newfs, mount, read) | FAIL | 132496 |
| `ufs-update` (in-place update, boot) | PASS | 189145 |
| `ufs-update-persist` (second update, boot) | PASS | 127471 |
| `ufs-full` (one-disk demo, rw root, tools) | PASS | 180066 |
| `ufs-statfs` (capacity through the wire block size) | PASS | 180015 |
| `make ufs-selftest` | PASS | ~1000 |
| `make check-fast` | PASSED | ~22000 |

## Expectation update (K413)

The `qemu-ufs` cases now assert the current product behaviour:

* **Styled boot screen (K374):** `R A V E N   B S D`, the 80-dash rule and
  `Welcome to RavenBSD 0.1 x86_64` before `login:`.
* **Getty restart (K234):** after `exit`, the case waits for `login:`, logs in a
  second time and re-proves its marker (`/persist-marker`,
  `/etc/ufs-update.txt`, the statfs fill file, or a one-disk echo). It forbids
  `init exited` and ends at a bounded 120-180 s timeout. The old
  `init exited (0)` step no longer matches the product: the demo getty restarts
  instead of letting init exit.

The product assertions (partition table, `ufs: on /`, markers, update contents,
network) are unchanged. `ufs-logout` keeps its timeout and two-login flow; only
the old single-line banner expectation was replaced.

## `disk-tools` — real defect, not test drift

`disk-tools` fails on every current tree tested with the same serial:

```text
devd: driver virtio-blk irq line=11
virtio-blk: registered disk
devd: driver virtio-net msix unavailable: bad handle or file descriptor; using legacy irq
devd: driver virtio-net launch failed: irq: resource busy
svcd: devd failed: exited before it became ready
blockd: waiting for disk:0: no such file or directory
svcd: blockd failed: readiness timeout
```

With a virtio-blk scratch disk and virtio-net attached, both functions expose
legacy PCI IRQ 11 (PIIX4 shared PIRQ); MSI-X setup returns EBADF, so both fall
back to the exclusive legacy GSI. The second claim returns `EBUSY`, `devd` exits,
and `blockd` never sees `disk:0`.

This is a product defect in the MSI-X fallback / shared-INTx path
(`userland/servers/devd`, `kernel/src/driver/irq.rs`), not a load artefact and
not a UFS error. It is outside the K413 test Owns, so the case keeps its full
assertions and the defect is recorded here for its owner.

## Transients

* `ufs-disk-ufs1` first run: `mount ufs: /mnt` produced no prompt for 30 s; the
  UFS1 image checked clean on the host and the re-run passed in 80 s.
* `ufs-update` first run: `smp: cpu 1 did not start` / `smp: 1 cpus online`, then
  QEMU exited before login (S159-class AP bring-up). The re-run passed in 189 s.

Both serials are kept in `tests/evidence/ufs/722d3109/serial/` as
`*-transient.log`.
