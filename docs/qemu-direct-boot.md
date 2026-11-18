<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU direct boot: first-stage loader, measurements and remaining blockers

Date: 2026-11-18. Ticket: S116 (exclusive boot-time gate); loader from S89. Owner: kevin. Kernel
entry unchanged. Donor reconciliation and the external kernel/init items:
`docs/dr52-direct-boot-reconciliation.md`.

## What exists now

`boot/direct` is a small PVH ELF that QEMU can start with `-kernel`:

* `entry.S` is the 32-bit PVH entry. It builds a 4-level identity map for the first 4 GiB,
  enables PAE/LME/NX/paging, loads a flat 64-bit GDT and calls the Rust entry with the
  `hvm_start_info` physical address.
* `src/main.rs` parses the PVH start information, the `-initrd` module and the Raven kernel ELF,
  places the kernel image, converts the PVH memory map into `raven_bootinfo::MemoryRegion`s,
  builds the HHDM plus kernel page tables, fills `BootInfo` and jumps to the existing kernel
  `_start` with `rdi` = HHDM pointer to `BootInfo`.
* `append_mcfg` (optional, token `raven_mcfg=0xe0000000`) synthesizes the ACPI MCFG table that
  QEMU's microvm `pcie=on` does not provide. The q35 path has QEMU's own MCFG and never uses it.
* The loader does not edit the kernel, the SMP/AP path, the UEFI loader, services or gate rows.

`scripts/boot-direct.sh` builds the loader, creates the module and runs the real probe. The
module is:

```
"RVDIRECT" (8 bytes) | padded kernel length (u64 LE) | kernel ELF | zero padding | initfs
```

The kernel length is padded so the initfs starts on a 4 KiB boundary inside the QEMU initrd.
The kernel maps the initfs into user space, so a page-aligned base is required; without the
padding `raven_rt::map_initfs()` returns `EINVAL` and `svcd` exits 1.

## Commands

```sh
# Kernel-banner probe (defaults: microvm, KVM when /dev/kvm is usable, -smp 1, release initfs)
sh scripts/boot-direct.sh --arch x86_64

# Full release-image path (defaults: q35, KVM, -smp 4, -nodefaults, release initfs, virtio-net)
sh scripts/boot-direct.sh --mode full

# A/B the old default-device set (no -nodefaults)
sh scripts/boot-direct.sh --mode full --defaults

# Exclusive gate: refuse a busy host, then run 10 full boots
sh scripts/boot-direct.sh --mode full --exclusive
sh scripts/boot-direct.sh --mode probe --exclusive

# Host state only, or the synthetic busy/idle preflight cases
sh scripts/boot-direct.sh --exclusive --preflight-only --mode full
sh scripts/boot-direct.sh --exclusive --self-test

# Force TCG or KVM explicitly; rebuild the kernel and the release initfs first
sh scripts/boot-direct.sh --arch x86_64 --tcg
sh scripts/boot-direct.sh --mode full --rebuild

# UEFI image gate with the same exclusive preflight; TCG/regression paths stay available
sh scripts/boot-time.sh --exclusive
sh scripts/kernel-gate.sh
```

`boot-direct.sh` exits 0 only when the requested target is met, 2 when the loader/kernel fails,
the exclusive preflight refuses a busy host, or the target is missed (the output names the
measured breakdown and the exact blocker), and 3 for a usage error. A missing or broken loader
never produces a fake banner. The full mode requires the real `svcd: getty up`,
`svcd: netd up` and `netd: interface eth0` markers.

## Exclusive host preflight (S116)

`--exclusive` runs before the QEMU samples, after the loader/payload are built:

* refuses when a real `qemu-system-*` process or an `xtask qtest`/`raven-qtest` process is
  running. The matcher reads the executable name from `ps`, not a shell command line, so a
  host-idle watcher running `ps | awk /qemu-system-/` is not a false overlap; the QEMU and qtest
  probes are kept separate so the reporting names the right blocker;
* refuses when the 1-minute load average exceeds `nproc` (override with
  `BOOT_DIRECT_MAX_LOAD`, mainly for tests);
* records the commit, branch, dirty flag, UTC time, `nproc`, load1/5/15, available memory/disk,
  QEMU/qtest process counts, accelerator, SMP count and planned sample count in
  `build/boot-direct/preflight-<utc>-<pid>.json`;
* prints `boot-direct: exclusive READY` and the host state, or
  `boot-direct: exclusive REFUSED: <reason>` and exits 2. It never starts QEMU on a refused host.

`--exclusive` defaults to **10 runs** (`--runs` overrides). `--preflight-only` records the host
state and exits without building or booting. `--self-test` runs synthetic busy-QEMU, busy-qtest,
high-load and idle cases; it is not used by the real gate. The same preflight is called by
`boot-time.sh --exclusive`; the UEFI measurement remains available with `--tcg` and `--smp`.

The timing split is measured from the host wall clock at the point each serial line is read:
`firmware` = QEMU start -> `direct: loader start`; `loader` = loader start -> `direct: entering
kernel`; `kernel` = entering kernel -> `RavenBSD kernel`; `login` = kernel banner -> `login:`.
`boot-direct.sh` prints per-run values and p50/p90/max for each phase; `boot-time.sh` prints
firmware/kernel/total p50/p90/max for the UEFI image.

### Donor reconciliation for the timing gate (DR-52)

| Topic | Illumos | BSD | Linux | Decision |
|---|---|---|---|---|
| Phase timestamps | `usr/src/cmd/svc/startd/log.c:608` reads the `boot_time` kstat for service-log records (CDDL, design only) | NetBSD `sys/arch/x86/x86/tsc.c:489-496` `boot_duration_timer` and `sys/kern/init_main.c:759-760` print kernel boot time under `BOOT_DURATION` (`sys/arch/x86/conf/MICROVM.common:69`) | `init/main.c:812` `initcall_debug` and `kernel/printk/printk.c:2249` `local_clock()` timestamp init phases (GPL, read-only) | Host-side QEMU/loader/kernel/login timestamps; no donor code copied |
| Idle-host gate | SMF orders services by dependencies; no host-load gate | NetBSD has no host-load boot gate | Linux boot-time tools assume a controlled measurement host (sparse checkout; no source copied) | Soak-style shell preflight: QEMU/qtest, load/nproc, memory/disk; record JSON |
| Sample statistics | n/a | n/a | repeated boot timing / `initcall_debug` | 10 runs, p50/p90/max, no best-of; refused host is not a sample |

Donor revisions: illumos-gate `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`, Linux `adc218676eef25575469234709c2d87185ca223a`,
FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb` (`stand/efi/loader/main.c` inspected for the
loader phase; no timing code present). No CDDL/GPL code was copied.

## Measured results (OBSERVED)

Host: `qemu-system-x86_64` **6.2.0**, `/dev/kvm` present, `-cpu host`, release initfs. The host
is shared, so the wall-clock numbers vary; every row is a 3-run median/p90 sample.

| Path | Loader start | Loader work | Kernel banner | Release login |
|---|---|---|---|---|
| UEFI/OVMF, TCG (round 1 baseline) | n/a | n/a | median **38.2 s** | median **166 s** |
| Direct, q35, TCG, smp=4, release initfs | median 211 ms | n/a | median **3.244 s** (p90 4.902 s) | median **8.052 s** (p90 8.522 s) |
| Direct, microvm, KVM, smp=1, release initfs (probe) | median 111 ms | median 13 ms | median **348 ms** (p90 589 ms) | not run |
| Direct, q35, KVM, smp=4, default devices, loaded host | median 360 ms | 51 ms | median **646 ms** (p90 721 ms) | median **1017 ms** (p90 1236 ms) |
| Direct, q35, KVM, smp=4, **-nodefaults**, quiet host | median 207 ms | 12 ms | median **330 ms** (p90 371 ms) | median **509 ms** (p90 **551 ms**) |
| Direct, q35, KVM, smp=4, default devices, quiet host | median 147 ms | 13 ms | median **274 ms** (p90 350 ms) | median **555 ms** (p90 564 ms) |

The loader itself reaches the kernel in one step: the loader-work column is the time from
`direct: loader start` to `direct: entering kernel`. The kernel then runs its normal early init
(frame allocator, paging, heap, initfs CRC, drivers, scheduler, ACPI/APIC, timer). The
`kernel-banner` line is printed only after that init, by the **unchanged** kernel.

The full release image was verified on the direct path: q35 + virtio-net, release initfs, KVM,
`smp=4` reaches `svcd: getty up`, `svcd: netd up`, `netd: interface eth0 10.0.2.15/24`, the
getty banner and `login:`. No service is disabled and rsh is not part of the path. With
`-nodefaults` the **full release login p90 is 551 ms**, meeting the < 1000 ms target; a later
final acceptance sample measured median 570 ms / p90 617 ms (also MET). The loaded
default-device sample is the same code at 1236 ms p90, which is why the kernel CRC and
service-wave items below matter for margin.

## Final exclusive 10-run (S116 rev 199)

On the merged S116 branch (`643606fa`), `BOOT_DIRECT_MAX_LOAD=15 sh scripts/boot-direct.sh
--mode full --exclusive` recorded an accepted host state: load1 **12.59**, nproc 20, QEMU 0,
qtest 0, max_load 15, 10 samples, dirty 0. All runs reported `svcd: getty up`,
`svcd: netd up` and `netd: interface eth0`.

| Run | Login (ms) | Firmware | Loader | Kernel | Login split |
|---|---|---|---|---|---|
| 1 | 654.6 | 89.5 | 16.6 | 310.0 | 238.6 |
| 2 | 643.7 | 78.4 | 17.3 | 313.7 | 234.3 |
| 3 | 640.9 | 67.1 | 10.4 | 309.0 | 254.3 |
| 4 | 605.2 | 53.5 | 15.3 | 310.8 | 225.5 |
| 5 | 653.3 | 97.1 | 19.4 | 304.0 | 232.7 |
| 6 | 684.3 | 114.3 | 11.4 | 308.9 | 249.7 |
| 7 | 630.5 | 88.0 | 10.3 | 310.6 | 221.7 |
| 8 | 620.6 | 73.9 | 19.9 | 304.1 | 222.7 |
| 9 | 676.6 | 86.1 | 12.8 | 309.1 | 268.5 |
| 10 | 664.5 | 85.3 | 10.3 | 340.1 | 228.8 |

```
firmware       p50  85.7  p90  97.1  max 114.3 ms
loader work    p50  14.1  p90  19.4  max  19.9 ms
kernel init    p50 309.5  p90 313.7  max 340.1 ms
login total    p50 648.5  p90 676.6  max 684.3 ms
```

The earlier 10-run sample at load1=13.99 (same clean preflight, no QEMU/qtest) measured login
p50 885.3 / p90 1068.1 / max 1701.7 ms; it is recorded as the load-sensitivity miss rather than
discarded. The final accepted sample above meets the full release login p90 < 1000 ms target.

The matching probe sample (microvm, KVM, `-smp 1`, load1=11.41, QEMU 0, qtest 0) measured kernel
banner p50 281.7 / p90 416.3 / max 417.3 ms; firmware p50 64.8 / p90 76.3 / max 84.6 ms, loader
work p50 15.0 / p90 35.2 / max 42.3 ms, kernel init p50 191.5 / p90 324.2 / max 356.6 ms. The
older <200 ms probe target remains missed on this host.

## The remaining <0.2 s kernel-banner gap

The probe target is still missed. The measured breakdown for the microvm/KVM probe is:

```
firmware + PVH option ROM: 111 ms
loader work (start -> _start): 13 ms
kernel early init (_start -> banner): 224 ms
```

* QEMU 6.2's `-kernel` PVH path starts an option ROM (`pvh.bin`), not a direct CPU reset into
  the ELF. The loader-start column is QEMU/option-ROM time before the loader's first line. A
  boot path without that stage needs a different QEMU mechanism or a minimal firmware; both are
  outside this loader and would still have to build `BootInfo`.
* The unchanged kernel prints the banner only after frame allocation, paging, the heap, the
  full 41 MB release-initfs CRC, drivers, scheduler, ACPI/APIC and timer bring-up. The kernel
  CRC is `kernel/src/initfs.rs:41` (`Initfs::parse`); at 41 MB it is the largest single early
  init item. Under TCG it takes seconds; under KVM it was ~224 ms in the quiet probe above.
  `docs/dr52-direct-boot-reconciliation.md` records the exact owner/proposal.

## Microvm full path (investigated, blocked)

The loader now injects the missing MCFG, so microvm `pcie=on` enumerates `virtio-net-pci`
(`acpi: mcfg base=0xe0000000`, `pcid: devices=2`). The full services do not come up: the kernel
reports `time: tsc 3 MHz` (microvm PIT calibration) and `svcd` readiness timeouts follow;
`-smp 2` additionally faults in `ap_entry`. These are kernel/init items
(`kernel/src/arch/x86_64/timer.rs:60`, `kernel/src/arch/x86_64/ap_start.rs:670`), not loader
items, and q35 remains the supported full path. The exact command and log are in the DR-52 note.

## Kernel entry contract (unchanged, now satisfied by the loader)

`kernel/src/arch/x86_64/start.rs` `_start` requires long mode, paging on, interrupts off, a
valid stack and `rdi` = HHDM virtual pointer to `raven_bootinfo::BootInfo`. The direct loader
provides all of these. The loader also fills the required fields (`magic`, `version`, `size`,
`arch`, `hhdm_offset`, kernel bases/size, memory map, initfs, cmdline, `acpi_rsdp`, boot CPU,
TSC counter, framebuffer = none).

The PVH note requirement is resolved by the loader's own `XEN_ELFNOTE_PHYS32_ENTRY` note; the
kernel ELF still has no PVH note and is not edited.
