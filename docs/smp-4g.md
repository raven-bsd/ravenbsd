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

## Revision 256 closeout

The 4 GiB qtest is green at `origin/master` `e2fbc690` (K167/K163; branch `f5d74d89`):
`timeout 600 cargo xtask qtest --case smp-4g --arch x86_64 --no-build` passes with
`uname -a`, `ls /bin`, `ps --posix`, `du /tmp` and `init exited (0)`. The `smp: 4 cpus online`
expectation is marked unordered because the kernel prints that line before the banner on this
build. The qtest also carries the release network device so `devd` does not spend 60 s in a
readiness timeout.

The 5 KVM + 5 TCG script (`scripts/smp-4g.sh`) ran five clean KVM boots (login 3.4-4.3 s,
prompt 0.04-0.07 s) and TCG run 1 (login 7.5 s). **TCG run 2 stopped in `du /tmp` with no output
for 30 s**; this is the known K134 `du` traversal hang, outside S157's Owns. The script stops on
the first failure and preserves `build/smp-4g/20261119T151732Z/tcg-2.log`. The 4G/4CPU matrix is
therefore **5 KVM + 1 TCG green, TCG du blocked on K134/Tim**; no retry, no timeout increase.

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
