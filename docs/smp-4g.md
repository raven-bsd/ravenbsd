<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 4 GiB / 4 vCPU boot smoke (S157)

Status: **BLOCKED.** The guest reaches the 4 GiB memory map, four CPUs, login and the brush
prompt, but the first interactive command is parsed as NUL bytes and the qtest fails. The memory
map is not missing the high RAM; the failure is after login.

## Run

- Fixture/case: `tests/qtest/cases/smp-4g.toml` (`-m 4096 -smp 4`, release profile).
- The first cold qtest attempt hit the 420 s cap during the release prebuild:
  `timeout 420 cargo xtask qtest --case smp-4g --arch x86_64` -> exit 124, no QEMU.
- Prebuild measurements: release image **261.33 s**; `check-loader.sh` **248.06 s**.
- The prebuilt qtest reached the guest with
  `timeout 320 cargo xtask qtest --case smp-4g --arch x86_64 --no-build`.

Preserved serial/JSON:

- `build/s157/smp-4g-login-uname-fail.log`
- `build/s157/smp-4g-login-uname-fail.json`

## Revision 255 confirmation

K167 (`origin/master` `26de7aae`) fixes the command-name NUL failure: the 4G guest reached
`RavenBSD raven 0.1 RavenBSD 0.1 (BASE) #1: fc941e85 2026-11-19 x86_64` from `uname -a`. The
actual qtest then hit its 230 s cap before the remaining `ls`/`ps`/`du`/exit steps because the
`check-loader.sh` pre-step and the 4G `devd` readiness timeout (60 s) consume most of the cap.
This is a cap/loaded-host issue, not the former NUL parser failure; no timeout was raised.

## Revision 253 confirmation

The failure was reproduced on `origin/master` `63a6f3d3` (RM1/K158 merged) at branch head
`7ee0dbcd`: `smp: 4 cpus online`, `mem: 3991 MiB usable in 9 regions`, login, and the same
`uname -a` -> `command not found` NUL corruption. RM1/K158 did not change this path. Preserved
again at `build/s157/smp-4g-rev253-uname-nuls.log`.

## Observed serial

```
smp: 4 cpus online
mem: 3991 MiB usable in 9 regions
...
RavenBSD 0.1 (x86_64) (tty:console)
login: root
Password:
...
raven# uname -a
error: command not found: \0\0\0\0\0\0\0\0
```

The prompt and command echo are correct. The shell then reports a command name made of NUL bytes,
so the first interactive command after login is not parsed as `uname -a`. The qtest fails at
step 7 (`^RavenBSD raven 0\.1 .* x86_64$`) and never reaches `ls /bin`, `ps --posix` or
`du /tmp`.

## Classification

- Memory map: **4 GiB is visible** (`mem: 3991 MiB usable in 9 regions`); this is not the
  "only 512 MiB mapped" failure.
- CPUs: `smp: 4 cpus online`; no missing CPU.
- Kernel: no panic, exception, TLB timeout, OOM or allocator diagnostic in the serial.
- Guest/userspace: the 4G run completes login and prints the prompt. The command buffer becomes
  NUL bytes, so the first suspect is the high-frame allocation/mapping used by the rsh command
  buffer or the console RX delivery at 4G, with the rsh parser as the other side.

**Handoff:** memory/allocator owner Mark and Stuart for the 4G high-frame mapping path; Bob/Phil
for the rsh input buffer. The test was not weakened and no kernel patch was made in S157.

## Why the boot-loop script is absent

`scripts/smp-4g.sh` was not added because the interactive qtest already fails before the matrix
repeats the same path, and a failing default qtest must not be presented as a passing baseline.
The case remains in the branch as the reproduction; the boot matrix can be added after the 4G
command-buffer failure is fixed.

## DR-52 donor reconciliation

This is a first-party test; no donor code was copied. The mapping is the existing RavenBSD
`MAP`/initfs path; no Illumos, BSD or Linux implementation is imported.
