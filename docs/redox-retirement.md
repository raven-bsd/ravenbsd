# Redox retirement audit (S117)

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

Status: **OBSERVED** on `origin/master` `8a591ccf` (S117). Scope: every Redox-derived
`PROVENANCE.toml` import plus the Redox-derived design/toolchain references found in the tree.
No code is removed by this audit; the classifications below are the decommission decision for the
owners named in the ticket.

## 0. Identity (DR-63)

"Redox-derived" is a **provenance statement, not the project identity** (DR-63). RavenBSD is a
Rust microkernel with its own design and ABI. The only remaining footprint is the
`crates/raven-rmm` page-table mapper below; keep the MIT notices and `PROVENANCE.toml`, do not
claim "no Redox code" until the mapper is replaced, and do not rewrite the working critical mapper
just to remove a label. Replace on touch per this list, or as a separately approved post-0.1.0
task if zero Redox code is required.

## 1. Result in one table

| Item | Count | Class |
|---|---:|---|
| `raven-rmm` / `redox-kernel` imports | 8 | 2 **keep/frozen**, 6 **replace on touch**, 0 remove |
| `relibc`, `ion`, `redoxfs`, `redox-bootloader` imports | 0 | none present |
| Redox-derived code without a `PROVENANCE` entry | 1 (`raven-rmm/src/arch/aarch64.rs`) | **replace on touch + provenance gap** |
| Redox-derived docs/toolchain references | 5 | frozen design/provenance references; no new design input |

The ticket's assumed `raven-vmem`/scheme/store Redox entries are **not present** in this tree.
The actual Redox footprint is `crates/raven-rmm`; `crates/raven-vmem` is a first-party arena
allocator with no `PROVENANCE` import. `cargo nextest run -p raven-vmem` is still run as the
ticket's acceptance command, but the mapper proof is `cargo nextest run -p raven-rmm`.

## 2. The eight `redox-kernel` imports

All eight are tier A / MIT, copied 2026-11-09 from `redox-kernel`
`015af335c7bed8bfaa8c2ae8419ceba46fe0a227`; the retained licence is
`crates/raven-rmm/LICENSE-MIT-REDOX`. The destination is the `raven-rmm` crate
(owner: **Mark**, memory/paging); the only dependency is `kernel/Cargo.toml:36`, used by
`kernel/src/memory/paging.rs`, `aspace.rs`, `uspace.rs`, `mmio.rs`, `process/mod.rs`,
`ipc/capture.rs`, `sched/thread.rs` and `smp/selftest.rs`.

Tests/gates for every row: `cargo nextest run -p raven-rmm` (64 tests: 6 lib + 31 mapper + 27 aarch64),
`cargo check -p raven-kernel` (exit 0), the S85a TCG `-smp 4` / KVM `-smp 4/2` 50-iteration
stress (latest OBSERVED kernel paging evidence; not re-run in S117), and `make check-fast`.

| # | Destination | Upstream path | Risk | Why | Class |
|---:|---|---|---|---|---|
| 1 | `crates/raven-rmm/src/lib.rs` | `rmm/src/lib.rs` | medium (public types) | Adapted PhysAddr/VirtAddr/Flags/PageSize API; no active bug | **keep/frozen** |
| 2 | `crates/raven-rmm/src/arch/mod.rs` | `rmm/src/arch/mod.rs` | low | Module declaration; x86-64 backend only | **keep/frozen** |
| 3 | `crates/raven-rmm/src/arch/x86_64.rs` | `rmm/src/arch/x86_64.rs` | high (PTE ABI/canonical) | Constants/17-bit canonical check; TLB moved to the kernel trait | **replace on touch** |
| 4 | `crates/raven-rmm/src/page/mod.rs` | `rmm/src/page/mod.rs` | medium (API split) | Flush trait replaced by caller-supplied `TlbFlusher` | **replace on touch** |
| 5 | `crates/raven-rmm/src/page/entry.rs` | `rmm/src/page/entry.rs` | high (PTE accessors) | PhantomData removed; accessors adapted to PhysAddr/Flags | **replace on touch** |
| 6 | `crates/raven-rmm/src/page/flags.rs` | `rmm/src/page/flags.rs` | high (hardware ABI) | x86-64 PTE bits written explicitly; bitflags removed | **replace on touch** |
| 7 | `crates/raven-rmm/src/page/table.rs` | `rmm/src/page/table.rs` | high (walk/free) | PhysMemory trait replaces the global Arch | **replace on touch** |
| 8 | `crates/raven-rmm/src/page/mapper.rs` | `rmm/src/page/mapper.rs` | **critical** (W^X, huge pages, table free, TLB) | Map/protect/unmap and conflict checks are the kernel memory safety boundary | **replace on touch** |

No row is **remove**: all eight are linked into `raven-rmm`, compiled into `raven-kernel`, and
covered by the 64 `raven-rmm` tests. No code or provenance is deleted here (DR-53/DR-52).

### Unprovenanced Redox-derived code

`crates/raven-rmm/src/arch/aarch64.rs` describes itself as "cross-checked against the Redox `rmm`
aarch64 backend" but has **no `PROVENANCE.toml` entry**. It is first-party BSD-3 code with the
RavenBSD header, but the donor reference must be resolved by Mark/Stuart:
**classify as replace-on-touch until the origin is recorded**. The 27 `raven-rmm` aarch64 tests
are the current proof. This is a provenance gap, not a reason to add a new Redox import in S117.

## 3. DR-52 reconciliation for the replacement candidates

Donor revisions: NetBSD `36eddebcc36cb0775aa2962005b9c05419a6ff01`, FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`, Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, Linux
`adc218676eef25575469234709c2d87185ca223a` (read-only reference).

| Topic | Illumos finding | BSD file / licence | Linux finding | Decision |
|---|---|---|---|---|
| x86-64 page-table mapper / W^X | `usr/src/uts/common/vm/hat.h` (CDDL): HAT interface; the i86pc HAT implementation is not in this sparse checkout | NetBSD `sys/arch/x86/x86/pmap.c` RCS 1.429 (2-clause BSD), `sys/arch/x86/include/pmap_private.h` RCS 1.5 (2-clause BSD), `sys/arch/x86/x86/x86_tlb.c` RCS 1.22 (2-clause BSD); FreeBSD `pmap.c` is **4-clause NO-COPY** (see `docs/smp-tlb-shootdown.md`) | sparse checkout has no x86 arch; outcome reference only | **replace on touch** with the NetBSD files as the permissive source; no code copied now |
| aarch64 descriptor encoding | `common/vm/hat.h` HAT design (CDDL) | NetBSD `sys/arch/aarch64/aarch64/pmap.c` RCS 1.159 (2-clause BSD), `sys/arch/aarch64/include/pmap.h` | sparse checkout has only bcm2711 DTS, no arm64 MM sources; outcome reference only | **replace on touch + record provenance** (Mark/Stuart) |
| TLB flush protocol | `common/vm/hat.h` design; NetBSD/FreeBSD TLB shootdown docs in `docs/smp-tlb-shootdown.md` | NetBSD `x86_tlb.c` (2-clause BSD); FreeBSD `pmap.c` 4-clause NO-COPY | sparse checkout has no MM sources; outcome reference only | Kernel S83/S103 owns the real shootdown; `raven-rmm` only has the caller-supplied `TlbFlusher` trait, no replacement in S117 |
| IPC scheme router (doc reference) | Illumos Doors/file-descriptor model (CDDL, design-only) | NetBSD `sys/kern/uipc_syscalls.c`, `sys/kern/kern_descrip.c` (2-clause BSD) | sparse checkout has no `fs/` or `kernel/` sources; VFS/descriptor outcome only | `docs/ipc.md` is a frozen design reference; replace on the next IPC touch (Tim) |
| Toolchain libc Redox module | n/a (Rust toolchain, not kernel) | NetBSD/FreeBSD libc unix modules (permissive) | glibc/Rust libc unix module (read-only) | `toolchain/libc-0.2.185-provenance.md` and the patch copy `src/unix/redox/mod.rs` are **frozen, out of the three owners' kernel scope**; Kevin's toolchain follow-up |

## 4. Redox-derived docs / references (no new design input)

| Reference | What it is | Class / action |
|---|---|---|
| `README.md:3` "Rust microkernel (ours, Redox-derived)" | Design lineage claim | **frozen**; change the wording when the README is next touched (no code) |
| `docs/ipc.md:17` "scheme packet router is the Redox-shaped" | Design reference | **replace on touch** with the NetBSD/Illumos IPC design above (Tim) |
| `docs/pi4-emmc2.md:212-213` `redox-base` driver files | Donor reading reference, not a PROVENANCE import | **frozen / no-copy**; use NetBSD/FreeBSD SDHCI/MMC when the Pi 4 driver is touched (Mark/Stuart) |
| `toolchain/libc-0.2.185-provenance.md` + `toolchain/patches/0001-add-ravenbsd-targets.patch` | Copy of `src/unix/redox/mod.rs` shape into the patched libc | **frozen**; Kevin/toolchain follow-up to replace with the Linux/BSD libc module |
| `toolchain/README.md:99-137` Redox cfg sites | Rust target plumbing, not a donor import | **keep** (upstream Rust target support; no Redox implementation copied) |

## 5. Ordered decommission list

No removal is claimed: every import is live and tested. The order is "touch order", so the
highest-risk replacement happens first when that code is next changed. Owners follow the current
lane ownership: Mark memory/paging and TLB (S85b, S101/S102), Stuart kernel SMP (S83/S113), Tim
scheme IPC (S108/S114); Kevin owns the toolchain.

| Order | Owner | Item | Action | Proof required after replacement |
|---:|---|---|---|---|
| 0 | **Mark + Stuart** | `raven-rmm/src/arch/aarch64.rs` provenance gap | Record the origin or replace on touch with NetBSD `sys/arch/aarch64/aarch64/pmap.c`; do not add a new Redox import | `cargo nextest run -p raven-rmm` (27 aarch64 tests) + kernel aarch64 qtests |
| 1 | **Mark** | `page/mapper.rs` (critical) | Replace on the next mapper change with the NetBSD `pmap.c`/`pmap_private.h` design (BSD-2) | 64 `raven-rmm` tests + `cargo check -p raven-kernel` + S85a TCG/KVM 50/50 + kernel paging qtests |
| 2 | **Mark** | `page/table.rs`, `entry.rs`, `flags.rs`, `arch/x86_64.rs` | Same replacement as the mapper; keep the `PhysMemory`/`TlbFlusher` seams | Same as order 1, plus the aarch64 descriptor tests |
| 3 | **Stuart** | kernel users (`memory/paging.rs`, `aspace.rs`, `uspace.rs`, `process/mod.rs`, `ipc/capture.rs`, `sched/thread.rs`) | Verify the kernel glue after Mark's mapper replacement; no ownership transfer of `raven-rmm` | `cargo check -p raven-kernel`, `make check-fast`, kernel boot/paging qtests |
| 4 | **Tim** | `docs/ipc.md` Redox-shaped scheme reference | Replace the design reference on the next IPC/scheme touch with the NetBSD/Illumos model; no code import exists | IPC unit tests + `make check-fast` |
| 5 | **Mark + Stuart** | `docs/pi4-emmc2.md` `redox-base` driver references | Use NetBSD/FreeBSD SDHCI/MMC when the Pi 4 driver is touched; no Redox code copy | Pi 4/QEMU eMMC test when the driver lands |
| 6 | **Kevin** (toolchain, outside Mark/Stuart/Tim) | `toolchain/libc-0.2.185-provenance.md` / Redox libc module copy | Audit and replace with the Linux/BSD libc module in the toolchain follow-up | Toolchain patch + libc target tests |
| — | **Mark** | `raven-rmm/src/lib.rs`, `arch/mod.rs` | Keep/frozen: type/trait surface, no active bug | Existing `raven-rmm` tests |

## 6. Evidence

OBSERVED on `8a591ccf`:

* `PROVENANCE.toml` has 865 imports; exactly **8** have `upstream = "redox-kernel"`, all
  `raven-rmm`, all MIT/tier A. No `relibc`, `ion`, `redoxfs` or `redox-bootloader` import exists.
* `cargo nextest run -p raven-rmm -p raven-vmem` → **80 tests run, 80 passed** (64 rmm:
  6 lib + 31 mapper + 27 aarch64; 16 vmem).
* `cargo nextest run -p raven-vmem` (ticket acceptance) → **16 tests run, 16 passed**.
* `cargo check -p raven-kernel` → exit 0 (12 warnings, none from the mapper).
* `make check-fast` → **PASSED** (guard, fmt, clippy host, host tests, doc tests, profiles,
  provenance, docs).
* S85a TCG/KVM stress from an earlier run: 50/50 on TCG `-smp 4` and KVM `-smp 4`/`-smp 2`
  (the kernel paging/mapper integration path); not re-run in S117 and no code change here.

No code, import, kernel source or `PROVENANCE.toml` entry is changed by S117. The audit's
classification is the decommission decision; the actual replacement is a per-owner follow-up when
the item is touched.

## Scope note: desktop exception (DR-54)

This audit covers Redox **core** imports only. The post-0.1.0 desktop/compositor/Wayland track
may study Redox Orbital/Wayland and Haiku's app_server/Interface Kit with owner/reviewer approval
(DR-54); that exception does not reopen Redox as a kernel/libc/boot/base donor and does not change
any classification here.
