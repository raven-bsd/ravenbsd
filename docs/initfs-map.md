<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Initfs mapping: COW, W^X and fault cost (K140)

Status: **OBSERVED** on x86_64. The `initfs-map` qtest exercises the ordinary
`Handle::INITFS` `MAP` path used by the loader: a page-aligned read-only mapping, an RX mapping,
private COW (including a forked child), the rejected forms, and a 3 MiB first-touch run.

## Mapping behaviour

| Case | Result |
|---|---|
| Page-aligned `READ` map and byte compare | pass; byte equals the initfs source |
| Page-aligned `READ\|EXEC` map and byte compare | pass |
| `READ\|WRITE` private COW map | pass; first write copies the page |
| Child writes the COW page | pass; parent still sees the initfs byte |
| Parent writes after child | pass; second read-only mapping and `image[0]` stay unchanged |
| Unaligned offset | `EINVAL` |
| `SHARED` initfs map | `EINVAL` |
| `WRITE\|EXEC` map | `EINVAL` |
| MAP handle duplicated without `MAP` | `EACCES` via `HANDLE_TRANSFER` |
| MAP handle duplicated without `EXEC` | `EACCES` for an RX map |
| `PROTECT` adds write to a read-only initfs mapping | `EACCES`; no writable-in-place PTE |

The kernel change is limited to that last row: `UserAddressSpace::protect` rejects adding `WRITE`
to a read-only initfs-backed VMA with `EACCES`. A mapping created writable (`MAP READ\|WRITE`)
remains a private COW mapping; the source archive is never modified.

## Fault-cost sample

Fixture: map 3 MiB from initfs offset 0, then read the first byte of every 4 KiB page
(768 pages). The sample is reported by the fixture and the host load is recorded alongside it.

| Sample | Total | Per page | Host load1 | Host |
|---|---:|---:|---:|---|
| First pass | 404 us | ~0.53 us | not captured on that line | 20 vCPU i7-12700H |
| Final pass | 1,446 us | 1,883 ns | 35.08 | 20 vCPU i7-12700H |

The loaded-host sample is not an idle-host baseline; it is recorded as-is. The fault path does no
extra copy or zeroing: the read-only PTE points at the immutable initfs frame, and the first write
performs the COW copy. No per-page kernel lock was added.

## Reproduction

```sh
timeout 120 cargo xtask qtest --case initfs-map --arch x86_64
timeout 90 cargo xtask qtest --case kernel-paging --arch x86_64
timeout 150 make check-fast
```

The `initfs-map` fixture is standalone under `tests/qtest/cases/initfs-map-fixture/`. Its `pre`
builds the fixture and a 4 MiB `/blob` so the 3 MiB fault range is a valid initfs range. The
aarch64 variant was not run: this clone has the x86_64 case only; the same fixture package has an
aarch64 target branch, but no aarch64 initfs/case was exercised here. No board result is claimed.

## DR-52 donor reconciliation

| Topic | Illumos (design only) | BSD (behaviour/port) | Linux (read-only) | Decision |
|---|---|---|---|---|
| Private file COW | `seg_vn`/HAT COW and write-fault design only | NetBSD `uvm_fault.c`/`uvm_map.c` COW and `MAP_PRIVATE` behaviour (BSD-2, study) | `do_wp_page`/`MAP_PRIVATE` outcomes | First-party: read-only initfs PTE, COW on write; no donor code |
| Executable mapping and W^X | Illumos segment protection design only | NetBSD `uvm_map` protection checks (BSD-2, study) | `PROT_EXEC`/W^X outcomes | First-party `MAP`/`PROTECT` checks; no new mapping path |
| PROTECT write on immutable backing | Illumos `segvn` protection rules; design only | NetBSD `uvm_map_protect` behaviour (BSD-2, study) | `mprotect`/private-file outcomes | Reject adding write to read-only initfs with `EACCES`; writable initial maps remain COW |

No donor code was copied or translated. The fixture and kernel check are original RavenBSD code.
