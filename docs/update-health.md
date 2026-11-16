<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# A/B boot-health marker

Status: K27c implementation. The health record lives on the UFS root and `init` waits for the
required services' declared readiness. QEMU proves good/bad markers and the root writer on x86-64.
The aarch64 UFS-root chain is not runnable yet: `devd` has no `pci:` on aarch64 `virt`, so K27b's
aarch64 testkernel case is superseded by this UFS-root case. Real-board power-loss persistence is
UNVERIFIED.

## Marker protocol

The update planner writes the trial slot and a 32-bit `trial_token` into `raven/boot.toml` and puts
the health record in the trial slot's UFS root:

```
slot = "b"
token = "deadbeef"
lba = "34"
```

The record is `/etc/boot-health` in the root filesystem (mounted at `/root` by `init`). Images
without a UFS root keep the legacy copy in the trial initfs; `init` prefers the root record.
`init` writes one 512-byte sector at the reserved LBA after all required services reach their
declared readiness. The sector is:

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
partition and is reserved for this marker; in the update-health image it sits on the UFS root disk
before partition 1. The loader scans every UEFI block device and accepts only a marker whose slot,
token and healthy state match the current `boot.toml` trial token.

Readiness is per service, from the manifests: every enabled service with `required = true` must
have reached its declared `readiness` (`none`, `scheme` or `exit`) before `init` writes the marker.
Optional services do not block it. A required service that fails before readiness, times out or
never starts leaves the trial unconfirmed.

## Loader transitions

1. On a trial boot, read the marker. A match confirms the slot immediately and clears the trial.
2. Without a match, load the trial kernel. A failed load decrements the budget and retries.
3. A successful load leaves the trial active and decrements the budget. `init` then writes the
   marker; the next boot confirms it.
4. If the budget reaches zero with no matching marker, clear the trial and boot the confirmed slot.

The marker is read-only from the loader's perspective; only `init` writes it, through the `block:`
scheme. The loader keeps the existing legacy path when `boot.toml` has no slot keys.

## QEMU evidence

* `update-health` — the fixture disk carries an ESP with slot A/B and a UFS root partition. The
  healthy marker on the root disk matches `boot.toml`; the loader logs
  `health marker matches slot b; confirmed`, boots slot B's real kernel, `init` mounts the UFS root,
  the root `health-probe` (readiness `exit`) reports ready, and `init` logs
  `required services ready`, `health marker written`, `settled` and `exited (0)`.
* `update-health-bad` — the marker is in trial state (stale) and slot B is an invalid kernel. The
  loader exhausts the budget and boots confirmed slot A's test kernel.
* `update-health-init` — the marker starts stale but slot B is valid. The loader boots it and logs
  `waiting for the health marker`; `init` mounts the UFS root, waits for the required root service,
  writes the healthy marker (slot B, token `3137624c`, state 1 at LBA 34) and exits 0.

`--arch aarch64` builds the same fixture but stops at `devd: pci: no such file or directory` on
aarch64 `virt`, so the aarch64 UFS-root chain remains UNVERIFIED until that platform path exists.

## Pi 4 image layout

`cargo xtask update --arch aarch64 --platform pi4 --image build/aarch64/ravenbsd-pi4.img` keeps the
Pi firmware files, `config.txt`, the DTB and the overlays in the FAT boot partition. It adds:

| Path | Content |
|---|---|
| `kernel8.img` | trial flat image the firmware loads |
| `raven/boot.toml` | confirmed slot, trial slot, token and remaining attempts |
| `raven/slot-a/kernel8.img` | slot A flat image |
| `raven/slot-b/kernel8.img` | slot B flat image |

Each slot image is the kernel ELF's load segments flattened at `0x80000` with the health-injected
initfs appended, matching the Pi 4 image builder. The planner requires `--arch aarch64`, defaults
to the `pi4` profile unless `--profile` is given, and preserves the base image's firmware files. A
supplied `--kernel` must be a Pi 4 kernel (entry `0x80000`, built with `RAVEN_PI4=1`); the trial
image build uses the normal Pi image builder, so `RAVEN_RPI_FIRMWARE` or `make setup` must resolve
the firmware directory. The Pi 4 image has no UFS root yet, so its trial initfs keeps the legacy
health record; the UFS-root path above applies to the update-health fixture. The Pi firmware always
loads the root `kernel8.img`; it does not read `raven/boot.toml`, so automatic slot selection and
rollback need a Pi-side consumer that does not exist yet. Selecting the trial on a board is
therefore UNVERIFIED.

## Board steps (UNVERIFIED)

Write the A/B image to SD/USB, boot the trial, let `init` reach all required services ready, then
cut power or reboot. Confirm the next boot logs the health match and keeps the trial slot. Repeat
with an interrupted required service and confirm the budget rolls back to the confirmed slot. On
the Pi 4 the
planner stages the slots and metadata but the firmware still boots the root image; a board run must
first provide the Pi-side consumer. No hardware run was performed.
