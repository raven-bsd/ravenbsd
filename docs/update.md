<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# A/B image updates

Status: K27 implementation. QEMU rollback is the acceptance; real-board rollback remains
UNVERIFIED.

## Layout

An updated x86-64/aarch64 image has three GPT partitions:

| Partition | Type | Content |
|---|---|---|
| ESP | EFI System | loader, `raven/boot.toml`, `raven/slot-a/**`, `raven/slot-b/**` |
| ROOT-A | Linux data | slot A payload placeholder (UFS root work follows) |
| ROOT-B | Linux data | slot B payload placeholder (UFS root work follows) |

The ESP slot directories carry the kernel ELF and initfs that the loader boots today. The root
partitions reserve the two future UFS roots; until the UFS root is populated the loader does not
read them. A legacy image without slot directories and without slot keys in `boot.toml` keeps the
old single-slot behaviour.

## `raven/boot.toml`

```
cmdline = "console=ttyS0 test-exit=1"
log_level = "info"
slot = "a"
trial = "b"
attempts = "2"
```

* `slot` is the confirmed (known-good) slot.
* `trial` is the slot under test; empty or absent means no trial.
* `attempts` is the remaining trial-boot budget (0..=255).

A file without `slot`, `trial` or `attempts` is legacy and the loader boots the legacy
`raven/kernel.elf` and `raven/initfs.img`.

## Loader transitions

The loader tries the trial slot up to `attempts` times:

1. Load the trial kernel and initfs. On success mark the trial healthy: it becomes `slot`, the
   trial is cleared and `attempts` is reset. A successful image load is the in-scope health
   check; a userland confirmation channel is future work.
2. On a failed load, log the failure, decrement `attempts`, rewrite `boot.toml` and retry.
3. When the budget reaches zero, clear the trial, keep the confirmed `slot`, rewrite `boot.toml`
   and boot the confirmed slot. This is the rollback path.
4. A trial with `attempts = 0` rolls back immediately.

Every transition is persisted before the kernel is entered, so a reset or power loss during the
trial cannot lose the confirmed slot. The rollback writes only when the slot store volume is
mounted; a corrupt or missing store leaves the legacy path.

## Planner

```
cargo xtask update --help
cargo xtask update --image build/x86_64/ravenbsd-x86_64.img --profile base
cargo xtask update --kernel <elf> --initfs <img> --attempts 2
```

With no existing image the planner first builds a normal image, then copies the confirmed slot
forward, installs the new kernel/initfs into the inactive slot, rewrites `boot.toml` with
`slot = <confirmed>`, `trial = <inactive>` and `attempts`, and rewrites the GPT with `ROOT-A`
and `ROOT-B`. The output image is deterministic for the same inputs and `SOURCE_DATE_EPOCH`.

The QEMU rollback fixture is a smaller planner mode used by `update-rollback`:

```
cargo xtask update --fixture build/qtest/update-rollback-slots.img \
  --kernel <good-testkernel> --initfs <initfs>
```

It writes a GPT disk whose ESP has slot A (good) and slot B (an invalid kernel). The qtest
attaches it as a second disk, so the normal image builder stays single-slot and the loader's A/B
selection is exercised without a special profile.

## Rollback evidence

`tests/qtest/cases/update-rollback.toml` expects the loader to log two failed slot-B attempts, the
rollback to slot A and the slot-A test kernel's `TESTKERNEL OK` line. The case exits success only
when the rollback boots the previous slot.

Real-board steps (UNVERIFIED): write the image to SD/USB, boot, interrupt power during the trial
window, boot again and confirm the loader selects the confirmed slot. QEMU `-no-reboot` exits on
a guest reset, so the QEMU case proves the same transition in one boot; a board run is required
for the literal power-cut/reboot variant.
