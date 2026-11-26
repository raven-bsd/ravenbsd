<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Power control

K235 adds the native power/reset primitive and the `/sbin` commands that use it.

## Kernel ABI

`proc_reboot(how: u32) -> !` is native syscall `0x46`. It is root-only (`euid == 0`).

| Flag | Value | Action |
|---|---:|---|
| `RB_AUTOBOOT` | `0` | normal reset |
| `RB_HALT` | `0x0000_0008` | halt without powering off |
| `RB_POWER_OFF` | `0x0000_4000` | power off |

Unknown bits return `EINVAL`; a non-root caller gets `EPERM`. The call does not return on a
successful platform action.

## Commands

| Command | Action |
|---|---|
| `/sbin/shutdown -r now` | stop services, then normal reset |
| `/sbin/shutdown -h now` | stop services, then halt |
| `/sbin/shutdown -p now` | stop services, then power off |
| `/sbin/reboot` | stop services, then normal reset |
| `/sbin/poweroff` | stop services, then power off |

The commands ask `svcd` to stop every service except the live getty session, flush file systems
through the `vfsd` `SYNC` request, and then call `PROC_REBOOT`. The getty/shell session is
terminated by the platform reset or poweroff after the other services have stopped. The commands
require effective uid 0.

## Platform

### QEMU x86-64

The kernel reads the ACPI FADT. Poweroff writes the S5 sleep type plus `SLP_EN` to the PM1a
(and PM1b when present) control block. Reset writes the FADT reset register and value; when the
firmware does not advertise one, it falls back to the conventional hard-reset sequence on I/O
port `0xcf9`.

QEMU q35 implements both paths, so the guest powers off or reboots without the host killing the
process. `scripts/qemu-ufs-run.sh` no longer passes `-no-reboot`.

### aarch64

QEMU `virt` uses PSCI `SYSTEM_OFF` and `SYSTEM_RESET`. The existing PSCI conduit (SMC or HVC
selected from the device tree) drives both.

### Raspberry Pi 4

PSCI `SYSTEM_OFF`/`SYSTEM_RESET` may be unavailable on a real Pi 4 board. The kernel falls back
to halt rather than pretending the board powered off; real Pi 4 watchdog/PMIC power control is a
named hardware blocker for this ticket. No Pi 4 board result is claimed.

## Tests

`tests/qtest/cases/poweroff.toml` logs in, runs `/sbin/shutdown -p now`, and requires the QEMU
process to end without a timeout. `tests/qtest/cases/reboot.toml` reboots the guest and requires a
second boot and login on the same image before powering it off. Both use the release image and
the root login.
