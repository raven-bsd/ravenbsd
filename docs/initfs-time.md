<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# initfs modification times (K162)

`ls -l` showed every initfs file as `Jan  1 00:00` while `/tmp` showed the real time: the archive
carried no modification time and `initfsd` returned `Stat::default()`.

## Format

The initfs format is now version 2. The fixed 64-byte header and 48-byte entries keep their size;
the former reserved bytes carry the times, so the layout, CRCs and data offsets are unchanged:

| Field | Offset | Meaning |
|---|---:|---|
| header epoch | 48 (u64) | image build time, seconds since the Unix epoch |
| header reserved | 56..64 | must be zero |
| entry mtime | 32 (u64) | per-file time; zero inherits the header epoch |
| entry reserved | 40..48 | must be zero |

The reader still accepts version 1 images: their reserved bytes must be zero, and every entry gets
the documented `DEFAULT_EPOCH` (`2026-01-01T00:00:00Z`). New images always carry a non-zero epoch.
The reader exposes the effective time as `Entry::mtime` and `Initfs::image_epoch()`.

## Reproducible build time

`InitfsBuilder::with_image_epoch(epoch)` records the image time. Files added with `add_file`
inherit it; `add_file_with_mtime` can set an explicit per-file time. The xtask image and initfs
paths use:

1. `SOURCE_DATE_EPOCH` when set (the reproducible A/B builds), else
2. the commit timestamp from the repository HEAD, else
3. `DEFAULT_EPOCH`.

The host clock is never read, so two builds of the same inputs are byte-identical.

## Stat path

`initfsd` copies `Entry::mtime` into the file node and returns it as `mtime`/`ctime`/`atime`.
Directories are synthesized from file paths and inherit the image epoch, so `/` and `/bin` are not
1970 either. `/tmp` (ramfs) and UFS stay on their own time paths and are unchanged by this ticket.

## Tests and evidence

* `crates/raven-initfs/src/builder_ut.rs`: image epoch inheritance, explicit entry mtime, the
  default epoch, and same-epoch byte equality / different-epoch change confined to time.
* `crates/raven-initfs/src/reader_ut.rs`: version 2 reads the header epoch and entry mtime,
  version 1 falls back to the default epoch, and nonzero reserved bytes are rejected.
* `userland/servers/initfsd/src/model.rs`: the stat model test fixture carries a time.
* `tests/qtest/cases/initfs-time.toml`: boots the shell-test image, stats `/init`, `/bin/ls` and
  `/bin`, and requires the same non-zero epoch.

The acceptance transcript, the A/B image hashes and the before/after `ls -l` evidence are in
`tickets/kevin/reports/K162.md`.
