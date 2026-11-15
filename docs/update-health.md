<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# A/B boot-health marker

Status: K27a implementation. QEMU proves the loader consumes good and bad markers; real-board
power-loss persistence is UNVERIFIED.

## Marker protocol

The update planner writes the trial slot and a 32-bit `trial_token` into `raven/boot.toml`, then
adds `/etc/boot-health` to that slot's initfs:

```
slot = "b"
token = "deadbeef"
lba = "34"
```

`init` writes one 512-byte sector at the reserved LBA after every service is up. The sector is:

| Offset | Field |
|---|---|
| 0..8 | `RAVENHLT` |
| 8..10 | version (1, little-endian) |
| 10 | slot (0 = A, 1 = B) |
| 11 | state (1 = healthy, 2 = trial) |
| 12..16 | trial token (little-endian) |
| 20..24 | FNV-1a checksum of bytes 0..20 |
| 24..512 | zero |

LBA 34 is the first GPT-usable sector (the primary entry array ends at 33). It is outside every
partition and is reserved for this marker. The loader scans every UEFI block device and accepts
only a marker whose slot, token and healthy state match the current `boot.toml` trial token.

## Loader transitions

1. On a trial boot, read the marker. A match confirms the slot immediately and clears the trial.
2. Without a match, load the trial kernel. A failed load decrements the budget and retries.
3. A successful load leaves the trial active and decrements the budget. `init` then writes the
   marker; the next boot confirms it.
4. If the budget reaches zero with no matching marker, clear the trial and boot the confirmed slot.

The marker is read-only from the loader's perspective; only `init` writes it, through the `block:`
scheme. The loader keeps the existing legacy path when `boot.toml` has no slot keys.

## QEMU evidence

* `update-health` — a second GPT disk carries slot A, a valid slot B and a matching marker. The
  loader logs `health marker matches slot b; confirmed` and boots the slot-B test kernel.
* `update-health-bad` — the marker is in trial state (stale) and slot B is an invalid kernel. The
  loader exhausts the budget and boots confirmed slot A.
* `update-health-init` — the real kernel boots the `update-health` profile; `init` reaches the
  all-services-up point, writes the marker through blockd to an attached virtio-blk GPT disk and
  logs `svcd: health marker written`, then `svcd: settled` and `init exited (0)`.

## Board steps (UNVERIFIED)

Write the A/B image to SD/USB, boot the trial, let `init` reach all services up, then cut power or
reboot. Confirm the next boot logs the health match and keeps the trial slot. Repeat with an
interrupted service set and confirm the budget rolls back to the confirmed slot. No hardware run
was performed.
