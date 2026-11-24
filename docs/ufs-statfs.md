<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# UFS statfs capacity (K181)

Status: **OBSERVED at `90c9d48d`.** The QEMU UFS2 root is a 256 MiB payload. Before K181,
`df -h` printed `ufs: 1.9G 296M 1.6G 16% /`; the UFS block count was multiplied by the 8x larger
filesystem block size. The fixed image reports `240M`, which is the payload's data-fragment
capacity after UFS metadata.

## 1. Raw fields

`ufs-core::ffs_vfsops::statfs` counts `blocks` and `blocks_free` in `fs_fsize` fragments:

```text
fragment_size = fs.fsize = 512
block_size    = fs.bsize = 4096
blocks        = fs.dsize = ~489760 fragments
blocks_free   = nbfree * frag + nffree fragments
```

The wire `StatFs` record has a single `block_size` field. `fsd-ufs` previously copied
`stat.block_size` (4096) into it while keeping the fragment count. libc `statfs` then filled
`f_bsize = f_frsize = 4096` and uutils `df` used `f_frsize`, producing:

```text
489760 fragments * 4096 = 2,006,056,960 bytes = 1.87G  (df rounds to 1.9G)
```

The correct mapping is to carry the fragment size because that is the unit of the count:

```text
wire.block_size = stat.fragment_size = 512
489760 fragments * 512 = 250,757,120 bytes = 239.1 MiB  (df -h: 240M)
```

Post-fix `df -a`:

```text
ufs:   244880   37761   207119  16% /
```

The 1 KiB-block total is 250,757,120 bytes, matching the UFS data fragments. The difference from
the 256 MiB image size is reserved/metadata space, not a unit error. The pre-fix `df -a` was not
captured; its `df -h` line and the raw field arithmetic above identify the 8x error.

## 2. Fix

`userland/servers/fsd-ufs/src/view.rs::statfs` now maps `block_size: stat.fragment_size`. The
wire `blocks` field and all free counts stay in fragments, so `df`, `statvfs` and the mount
capacity describe the same bytes. No protocol, kernel or UFS-base change; the mount name remains
`ufs:` and `/`.

`stat -f /` is not usable on this image: uutils `fsext::fs_type()` is still `unimplemented!()` for
`target_os = "ravenbsd"` and panics. That path is outside this ticket's owns (`uucore`, not
`uu/df`); `df` and the raw field arithmetic above are the evidence.

## 3. Regression

`tests/qtest/cases/ufs-statfs.toml` boots the K176 `qemu-ufs` image, logs in, and checks:

- `df -h /` reports `ufs:` with a size in the 200-249 MiB range (pre-fix 1.9G fails);
- `df -a` reports the 1 KiB-block total for the same filesystem;
- a file written to the root changes the usage column without changing the capacity;
- `mount` still reports `ufs: on /`;
- no boot/pivot panic.

Observed post-fix:

```text
$ timeout 300 cargo xtask qtest --case ufs-statfs --arch x86_64
ufs-statfs: pass                 # duration_ms 203548
ufs:            240M   37M  203M  16% /
ufs:              244880 37761    207119  16% /
ufs: on /
```

The pre-fix run of the same case failed on the `200-249M` expectation and the raw `df` line was
`ufs: 1.9G 296M 1.6G 16% /`; the pre-fix ESP and root disk were built from `90c9d48d`.

## 4. Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| UFS statfs units | UFS `statvfs` reports block units per filesystem; behaviour only (CDDL, no copy). | NetBSD FFS `ffs_statfs` reports `f_bsize`/`f_frsize` with counts in fragment units; the existing `ufs-core` is the NetBSD-derived BSD-3-Clause port. | GNU `df` uses `f_frsize` when present; outcome only (GPL, no copy). | Keep the UFS count in fragments and put `fragment_size` in the single wire size field. |
| Capacity display | Not applicable. | `df` outcome only. | `df` output contract only. | Preserve `ufs:`/`/` names and the 1 KiB `df -a` contract; no fake capacity. |

No donor source was copied. The clone has no `donors.lock`, so no donor revision is quoted.
