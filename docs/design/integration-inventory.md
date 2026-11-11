<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Integration inventory

What the tree contains, where each package actually runs, who consumes it, and where consumer
integration is still missing. The table below is generated from the repository by
`tools/integration-inventory`; do not edit it by hand.

Reproduce it with:

```
cargo run -p raven-integration-inventory -- --root .        # print the table
cargo run -p raven-integration-inventory -- --write         # refresh the block in this file
cargo run -p raven-integration-inventory -- --check         # exit 1 when this file is stale
```

## How to read the columns

* **kind** - `core library` (shared crate consumed by a target component), `host test mirror` (the
  target includes the source file directly with `#[path = ...]`, or the crate includes target
  sources to test them on the host), `tool` (host binary), `server` (`userland/servers/*`),
  `spike`, `kernel`, `loader`, `userspace program`, `test harness`, `test fixture`.
* **target execution** - `host test` when the package is built and tested on the host, `QEMU
  guest` when it ends up in a guest booted by a qtest case (directly, through a dependency or
  through a source include: a library runs wherever its consumers run), `image` when it is
  installed in a profile that no case boots.
* **consumers** - workspace packages with a normal or build dependency on it, packages that
  include its sources (`source included by`), packages whose sources it includes, profiles that
  install it, and the qtest cases that boot those profiles or build its fixture.
* **restart** - the service restart policy from the `/etc/services.d` manifest the profile
  installs for that server binary, or `-`.
* **known gap** - the integration finding for that package, or `-`.

## Counts

The generated block ends with the counts per kind and per target execution. They describe **what
exists**, not how mature it is: a high "host test" count only says that those packages are
compiled and tested on the host, it says nothing about coverage or quality. Line counts are
deliberately absent for the same reason.

<!-- integration-inventory:begin -->

```
| component | path | kind | target execution | consumers | restart | known gap |
|---|---|---|---|---|---|---|
| `bench-host` | `tools/bench-host` | tool | host test | - | - | - |
| `linux-corpus` | `tools/linux-corpus` | tool | host test | - | - | - |
| `linux-elf` | `crates/linux-elf` | core library | host test | - | - | host tests only; no target consumer |
| `linux-shim` | `userland/linux-shim` | userspace program | host test | - | - | host tests only; no target consumer |
| `raven-abi` | `crates/raven-abi` | core library | host test, QEMU guest | packages: raven-loader, raven-handles, raven-ipc, raven-linux-dispatch, raven-netcore, raven-proto-audio, raven-proto-fs, raven-proto-net, raven-scheme, raven-usercopy, raven-kernel, raven-echo-dialogue, raven-s08-init, raven-s09-futex, raven-s11-echo, raven-s21-edu, raven-s24-child, raven-s24-parent, raven-s25-xfer, raven-s26-evq, raven-s35-handles, raven-svcctl, raven-uname, raven-init, linux-shim, raven-rt, raven-coretest, raven-initfsd, raven-initfsd-probe, raven-logd, raven-nulld, raven-ramfsd, raven-ramfsd-probe, raven-randd, raven-sysd, raven-zerod | - | - |
| `raven-audio-core` | `crates/raven-audio-core` | core library | host test | - | - | host tests only; no target consumer |
| `raven-bootinfo` | `crates/raven-bootinfo` | core library | host test, QEMU guest | packages: raven-loader, raven-bootinfo-fdt, raven-kernel, raven-testkernel | - | - |
| `raven-bootinfo-fdt` | `crates/raven-bootinfo-fdt` | host test mirror | host test, QEMU guest | source included by: kernel | - | - |
| `raven-cmdline` | `crates/raven-cmdline` | host test mirror | host test, QEMU guest | source included by: kernel | - | - |
| `raven-conform` | `crates/raven-conform` | tool | host test | packages: raven-netcore-packetdrill-harness | - | - |
| `raven-coretest` | `userland/servers/coretest` | server | QEMU guest | profiles: core-test; cases: core-servers | on-failure | - |
| `raven-echo` | `userland/bin/echo` | userspace program | host test, QEMU guest | profiles: base, core-test, dev, initfsd-test, ramfsd-test, shell; cases: boot-uname, boot-uname-aarch64, core-servers, dialogue-echo, driver-dma, driver-irq, driver-mmio, evq, exc-div0, exc-doublefault, exc-int3, exc-pagefault, futex-userspace, handle-stale, handle-transfer, initfsd, kernel-early-boot, kernel-early-boot-aarch64, kernel-exc-aarch64, kernel-paging, kernel-paging-aarch64, kernel-spawn, kernel-timer, kernel-timer-aarch64, kernel-usermode, loader-testkernel, ramfsd, sched-kernel-threads, sched-kernel-threads-aarch64, scheme-echo, smp-boot, user-fmt | - | - |
| `raven-echo-dialogue` | `tests/fixtures/echo-dialogue` | test fixture | - | - | - | no qtest case builds this fixture |
| `raven-elf` | `crates/raven-elf` | core library | host test, QEMU guest | packages: raven-loader, linux-elf, raven-kernel, raven-s11-echo, raven-s21-edu, raven-s24-parent, raven-s25-xfer, raven-s26-evq, raven-rt | - | - |
| `raven-esr` | `crates/raven-esr` | core library | host test, QEMU guest | packages: raven-kernel | - | - |
| `raven-evq-core` | `crates/raven-evq-core` | host test mirror | host test, QEMU guest | source included by: kernel | - | - |
| `raven-fdt-lite` | `crates/raven-fdt-lite` | host test mirror | host test, QEMU guest | packages: raven-bootinfo-fdt; source included by: kernel | - | - |
| `raven-frames` | `crates/raven-frames` | host test mirror | host test, QEMU guest | source included by: kernel | - | - |
| `raven-handles` | `crates/raven-handles` | core library | host test, QEMU guest | packages: raven-kernel | - | - |
| `raven-init` | `userland/init` | userspace program | host test, QEMU guest | profiles: base, core-test, dev, initfsd-test, ramfsd-test, shell, svcd-test, tiny; cases: all 33 | - | - |
| `raven-initfs` | `crates/raven-initfs` | core library | host test, QEMU guest | packages: raven-kernel, raven-s11-echo, raven-s21-edu, raven-s24-parent, raven-s25-xfer, raven-s26-evq, raven-init, raven-rt, raven-initfsd, xtask | - | - |
| `raven-initfsd` | `userland/servers/initfsd` | server | host test, QEMU guest | profiles: core-test, initfsd-test, ramfsd-test, shell; cases: core-servers, initfsd, ramfsd | on-failure | - |
| `raven-initfsd-probe` | `userland/servers/initfsd-probe` | server | QEMU guest | profiles: initfsd-test; cases: initfsd | on-failure | - |
| `raven-integration-inventory` | `tools/integration-inventory` | tool | host test | - | - | - |
| `raven-ipc` | `crates/raven-ipc` | host test mirror | QEMU guest | packages: raven-scheme, raven-s11-echo, raven-s25-xfer, raven-s26-evq; includes source: kernel | - | - |
| `raven-kernel` | `kernel` | kernel | host test, QEMU guest | source included by: crates/raven-ipc; includes source: crates/raven-bootinfo-fdt, crates/raven-cmdline, crates/raven-evq-core, crates/raven-fdt-lite, crates/raven-frames, crates/raven-msi; cases: all 33 | - | - |
| `raven-linux-abi` | `crates/raven-linux-abi` | core library | host test | packages: linux-elf, raven-linux-dispatch, linux-shim | - | host tests only; no target consumer |
| `raven-linux-dispatch` | `crates/raven-linux-dispatch` | core library | host test | - | - | host tests only; no target consumer |
| `raven-loader` | `boot/loader` | loader | host test, QEMU guest | cases: all 33 | - | - |
| `raven-logd` | `userland/servers/logd` | server | host test, QEMU guest | profiles: core-test, initfsd-test, ramfsd-test, shell; cases: core-servers, initfsd, ramfsd | on-failure | - |
| `raven-mem` | `crates/raven-mem` | core library | host test, QEMU guest | packages: raven-kernel | - | - |
| `raven-msi` | `crates/raven-msi` | host test mirror | host test, QEMU guest | source included by: kernel | - | - |
| `raven-netcore` | `crates/raven-netcore` | core library | host test | packages: raven-netcore-packetdrill-harness | - | host tests only; no target consumer |
| `raven-netcore-packetdrill-harness` | `tests/net/packetdrill-harness` | test harness | host test | - | - | - |
| `raven-nulld` | `userland/servers/nulld` | server | host test, QEMU guest | profiles: core-test, initfsd-test, ramfsd-test, shell; cases: core-servers, initfsd, ramfsd | on-failure | - |
| `raven-prng` | `crates/raven-prng` | core library | host test, QEMU guest | packages: raven-randd | - | - |
| `raven-proto-audio` | `crates/raven-proto-audio` | core library | host test | - | - | host tests only; no target consumer |
| `raven-proto-fs` | `crates/raven-proto-fs` | core library | host test, QEMU guest | packages: raven-ramfs-core, raven-scheme, raven-svcctl, raven-init, raven-rt, raven-initfsd, raven-initfsd-probe, raven-logd, raven-nulld, raven-ramfsd, raven-ramfsd-probe, raven-randd, raven-sysd, raven-zerod | - | - |
| `raven-proto-net` | `crates/raven-proto-net` | core library | host test | - | - | host tests only; no target consumer |
| `raven-proto-svc` | `crates/raven-proto-svc` | core library | host test, QEMU guest | packages: raven-svcctl, raven-init | - | - |
| `raven-provenance` | `crates/raven-provenance` | tool | host test | - | - | - |
| `raven-qtest` | `tests/qtest` | test harness | host test | - | - | - |
| `raven-ramfs-core` | `crates/raven-ramfs-core` | core library | host test, QEMU guest | packages: raven-ramfsd | - | - |
| `raven-ramfsd` | `userland/servers/ramfsd` | server | QEMU guest | profiles: core-test, initfsd-test, ramfsd-test, shell; cases: core-servers, initfsd, ramfsd | on-failure | - |
| `raven-ramfsd-probe` | `userland/servers/ramfsd-probe` | server | QEMU guest | profiles: ramfsd-test; cases: ramfsd | on-failure | - |
| `raven-randd` | `userland/servers/randd` | server | host test, QEMU guest | profiles: core-test, initfsd-test, ramfsd-test, shell; cases: core-servers, initfsd, ramfsd | on-failure | - |
| `raven-rmm` | `crates/raven-rmm` | core library | host test, QEMU guest | packages: raven-kernel | - | - |
| `raven-rt` | `userland/rt` | userspace program | host test, QEMU guest | packages: raven-s11-echo, raven-s21-edu, raven-s24-child, raven-s24-parent, raven-s25-xfer, raven-s26-evq, raven-s35-handles, raven-echo, raven-svcctl, raven-uname, raven-init, raven-testsvc, raven-coretest, raven-initfsd, raven-initfsd-probe, raven-logd, raven-nulld, raven-ramfsd, raven-ramfsd-probe, raven-randd, raven-sysd, raven-zerod | - | - |
| `raven-s08-init` | `tests/fixtures/s08-init` | test fixture | QEMU guest | cases: kernel-usermode | - | fixture for kernel-usermode |
| `raven-s09-futex` | `tests/fixtures/s09-futex` | test fixture | QEMU guest | cases: futex-userspace | - | fixture for futex-userspace |
| `raven-s11-echo` | `tests/fixtures/s11-echo` | test fixture | QEMU guest | cases: scheme-echo | - | fixture for scheme-echo |
| `raven-s21-edu` | `tests/fixtures/s21-edu` | test fixture | QEMU guest | cases: driver-dma, driver-irq, driver-mmio | - | fixture for driver-dma, driver-irq, driver-mmio |
| `raven-s24-child` | `tests/fixtures/s24-spawn/child` | test fixture | QEMU guest | cases: kernel-spawn | - | fixture for kernel-spawn |
| `raven-s24-parent` | `tests/fixtures/s24-spawn/parent` | test fixture | QEMU guest | cases: kernel-spawn | - | fixture for kernel-spawn |
| `raven-s25-xfer` | `tests/fixtures/s25-xfer` | test fixture | QEMU guest | cases: handle-transfer | - | fixture for handle-transfer |
| `raven-s26-evq` | `tests/fixtures/s26-evq` | test fixture | QEMU guest | cases: evq | - | fixture for evq |
| `raven-s35-handles` | `tests/fixtures/s35-handles` | test fixture | QEMU guest | cases: handle-stale | - | fixture for handle-stale |
| `raven-sched-core` | `crates/raven-sched-core` | core library | host test, QEMU guest | packages: raven-kernel | - | - |
| `raven-scheme` | `crates/raven-scheme` | core library | host test, QEMU guest | packages: raven-svcctl, raven-init, raven-rt, raven-coretest, raven-initfsd, raven-initfsd-probe, raven-logd, raven-nulld, raven-ramfsd, raven-ramfsd-probe, raven-randd, raven-sysd, raven-zerod | - | - |
| `raven-svc-manifest` | `crates/raven-svc-manifest` | core library | host test, QEMU guest | packages: raven-init | - | - |
| `raven-svcctl` | `userland/bin/svcctl` | userspace program | host test, QEMU guest | profiles: core-test, initfsd-test, ramfsd-test, shell; cases: core-servers, initfsd, ramfsd | - | - |
| `raven-sysd` | `userland/servers/sysd` | server | host test, QEMU guest | profiles: core-test, initfsd-test, ramfsd-test, shell; cases: core-servers, initfsd, ramfsd | on-failure | - |
| `raven-testkernel` | `tests/fixtures/testkernel` | test fixture | QEMU guest | cases: loader-testkernel | - | fixture for loader-testkernel |
| `raven-testsvc` | `userland/init/testsvc` | userspace program | QEMU guest | profiles: svcd-test; cases: svcd-test | on-failure | - |
| `raven-timemath` | `crates/raven-timemath` | core library | host test, QEMU guest | packages: raven-kernel | - | - |
| `raven-uname` | `userland/bin/uname` | userspace program | host test, QEMU guest | packages: raven-init; profiles: base, core-test, dev, initfsd-test, ramfsd-test, shell, tiny; cases: boot-uname, boot-uname-aarch64, core-servers, dialogue-echo, driver-dma, driver-irq, driver-mmio, evq, exc-div0, exc-doublefault, exc-int3, exc-pagefault, futex-userspace, handle-stale, handle-transfer, initfsd, kernel-early-boot, kernel-early-boot-aarch64, kernel-exc-aarch64, kernel-paging, kernel-paging-aarch64, kernel-spawn, kernel-timer, kernel-timer-aarch64, kernel-usermode, loader-testkernel, ramfsd, sched-kernel-threads, sched-kernel-threads-aarch64, scheme-echo, smp-boot, user-fmt | - | - |
| `raven-usercopy` | `crates/raven-usercopy` | core library | host test, QEMU guest | packages: raven-kernel | - | - |
| `raven-va-spike` | `libc/spike/va` | spike | host test | - | - | host tests only; no target consumer |
| `raven-vma` | `crates/raven-vma` | core library | host test, QEMU guest | packages: raven-kernel | - | - |
| `raven-zerod` | `userland/servers/zerod` | server | host test, QEMU guest | profiles: core-test, initfsd-test, ramfsd-test, shell; cases: core-servers, initfsd, ramfsd | on-failure | - |
| `ufs-core` | `crates/ufs-core` | core library | host test | packages: ufs-oracle-tests, ufs-powercut-tests, ufs-fsck, ufs-mkfs | - | host tests only; no target consumer |
| `ufs-fsck` | `tools/ufs-fsck` | tool | host test | - | - | - |
| `ufs-mkfs` | `tools/ufs-mkfs` | tool | host test | packages: xtask | - | - |
| `ufs-oracle-tests` | `tests/ufs-oracle` | test harness | host test | - | - | - |
| `ufs-powercut-tests` | `tests/ufs-powercut` | test harness | host test | - | - | - |
| `xtask` | `xtask` | tool | host test | - | - | - |
```

```
packages: 76
by kind: core library 26; host test mirror 7; kernel 1; loader 1; server 10; spike 1; test fixture 11; test harness 4; tool 8; userspace program 7;
by target execution: - 1; QEMU guest 53; host test 59;
```

<!-- integration-inventory:end -->

## Top 10 packages without a target consumer

These are the rows whose `known gap` is `host tests only; no target consumer`: the code exists
and is host-tested, but no target component (kernel, loader, server, userspace program) builds it
into an image. Owning tickets use existing ids only; `-` means the board has no ticket for that
integration yet.

| # | package | path | what is missing | owning ticket |
|---|---|---|---|---|
| 1 | `ufs-core` | `crates/ufs-core` | the UFS read/write core has no server or kernel consumer; only `ufs-mkfs`, `ufs-fsck` and the two test harnesses use it | - (filesystem server after K67; mel lane) |
| 2 | `raven-netcore` | `crates/raven-netcore` | the socket core has no network server or NIC driver consumer; only the packetdrill harness uses it | - (B07 delivered the core; the driver/server is unscheduled) |
| 3 | `raven-proto-net` | `crates/raven-proto-net` | the socket protocol has no server consumer | - (same integration as #2) |
| 4 | `raven-audio-core` | `crates/raven-audio-core` | the PCM core has no audio server consumer | - (B53 delivered the core; the server is unscheduled) |
| 5 | `raven-proto-audio` | `crates/raven-proto-audio` | the PCM stream protocol has no server or driver consumer | - (same integration as #4) |
| 6 | `linux-elf` | `crates/linux-elf` | the Linux ELF loader rules are not used by the guest; nothing execs a Linux binary yet | - (B08; needs the personality server) |
| 7 | `raven-linux-abi` | `crates/raven-linux-abi` | the syscall tables and structs are consumed only by host crates | - (B06; needs the personality server) |
| 8 | `raven-linux-dispatch` | `crates/raven-linux-dispatch` | the dispatch table is consumed only by host crates | - (B10; needs the personality server) |
| 9 | `linux-shim` | `userland/linux-shim` | the in-process fast paths are host-tested only; no server loads them | - (B11; needs the personality server) |
| 10 | `raven-va-spike` | `libc/spike/va` | the spike is unused by design | - (spike; keep as a reference) |

The shape of this list is the finding: filesystems (UFS), networking, audio and Linux
compatibility are concentrated in host cores and tooling; the guest has protocol crates
(`raven-proto-fs`, `raven-proto-svc`, `raven-scheme`) and server binaries, but no server or driver
turns the network, audio, UFS or Linux pieces into boot-time consumers. Kernel-side integration
that already exists counts: for example `raven-fdt-lite`, `raven-frames`, `raven-cmdline`,
`raven-evq-core` and `raven-msi` are *mirrors* (the kernel includes their source), which is why
they do not appear in this list.

## What this does not prove

* It does not prove that a package is unused. Host-test mirrors, included source modules and
  dev-only harnesses are legitimate consumers with no normal dependency edge; the `kind` and
  `source included by` columns record them. Treat a `-` restart or an empty consumers cell as "no
  consumer of the kinds this tool reads", not as "dead code".
* It does not measure coverage, quality or maturity, and the counts are not a score.
* It reads manifests, profile fragments, service manifests (only `exec` and `restart`) and qtest
  cases. Anything wired outside those files (a shell script that copies a binary, a Makefile rule)
  is invisible to it.
* "QEMU guest" means a case *boots a profile or image that contains the package*; it does not mean
  the case exercises the package's code paths.
* The restart column is the manifest default; a profile's `rc.conf` may override it at boot. The
  one-shot profile gate (`B67`) is what enforces those overrides.
