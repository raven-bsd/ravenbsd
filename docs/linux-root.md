<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Linux root recipe (B32)

Status: B32, 2026-11-15. Evidence tags: **OBSERVED** (run on the target or read from the source),
**PROPOSED** (not implemented), **UNVERIFIED** (not exercised).

## 1. What this adds

`linuxd` runs static images from the initfs; the Linux corpus needs a real root tree. This ticket
adds the add-on recipe `addons/linux-root/`, which stages an Alpine minirootfs at
`build/linux-root/compat/linux` and builds the `linux-root` qtest. It is a recipe, not a product
binary: the tarball is user-supplied and never committed, and the recipe never downloads during a
build or a test. The product's Linux loader and its dynamic-interpreter path are B33's work.

## 2. Pinned source

| | |
|---|---|
| Source | Alpine Linux minirootfs |
| Version | 3.24.4 |
| Architecture | x86_64 |
| URL | https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/x86_64/alpine-minirootfs-3.24.4-x86_64.tar.gz |
| SHA-256 | `cdaa38cb8de3537967ee5b6c9b6cd00cc962da2b018e6eea48696bb274923012` |
| Size | 3,704,088 bytes |
| Licence | The minirootfs is a collection of Alpine packages, each with its own licence (recorded in `/lib/apk/db/installed` in the staged tree). BusyBox is GPL-2.0-only. The recipe, fixture and this document are BSD-3-Clause. |

The same URL, hash and size are in `addons/linux-root/recipe.env`,
`addons/linux-root/alpine-minirootfs-3.24.4-x86_64.tar.gz.sha256` and
`addons/linux-root/README.md`. `build.sh --check` fails if the copies drift.

## 3. Usage

```sh
sh addons/linux-root/build.sh --check    # pinned metadata; no tarball needed
sh addons/linux-root/build.sh --fetch    # optional, explicit network download
sh addons/linux-root/build.sh --stage    # verify and stage the tree
sh addons/linux-root/build.sh --qtest    # stage and build build/linux-root/initfs.img
```

The tarball is searched at `build/linux-root/downloads/<file>`, then
`addons/linux-root/downloads/<file>`, or the file named by `RAVEN_ALPINE_MINIROOTFS`. `--check`
verifies the hash, size and the three required members (`./bin/busybox`,
`./lib/ld-musl-x86_64.so.1`, `./etc/alpine-release`) when the tarball is present. `--stage` and
`--qtest` fail with a clear message when the user-supplied tarball is absent; they never download.
`--fetch` is the only network operation and is user-invoked.

## 4. Staged layout

```
build/linux-root/
├── downloads/alpine-minirootfs-3.24.4-x86_64.tar.gz   (user-supplied, gitignored)
├── compat/linux/...                                   (84 regular files, 8,467,516 bytes)
├── manifest.txt                                       (version, URL, hash, release, counts)
└── initfs.img                                         (qtest image, ~8.5 MiB)
```

The full tree is staged on disk for B33. The qtest initfs packs every regular file of the tree
under `/compat/linux/...`; the Alpine symlinks are not packed because the initfs format has no
symlink entries. The qtest's exact images are `/init` (the fixture), `/sbin/linuxd`, and the three
files the fixture reads; the rest is present for future loader work and for the file-count evidence.

The recipe does **not** copy any code from Alpine; it extracts a user-supplied binary rootfs. The
tree and tarball are outside git (`addons/linux-root/.gitignore` ignores `downloads/`; the root
`build/` is already ignored).

## 5. qtest and observed boundary

`tests/qtest/cases/linux-root.toml` boots the native fixture
`tests/qtest/cases/linux-root-fixture` as `/init`. The fixture:

1. parses the initfs and reads `/compat/linux/etc/alpine-release`,
   `/compat/linux/bin/busybox` and `/compat/linux/lib/ld-musl-x86_64.so.1`, printing their sizes;
2. spawns `/sbin/linuxd` with the exact argument vector
   `["/sbin/linuxd", "/compat/linux/bin/busybox", "ls"]`, so linuxd sees the Linux
   `argv = ["/compat/linux/bin/busybox", "ls"]`;
3. waits and records the Linux child's status.

**OBSERVED** serial output (`build/qtest/linux-root.log`):

```
linux-root: release=3.24.4
linux-root: busybox bytes=804616 loader bytes=670312
linux-root: attempt path=/compat/linux/bin/busybox argv1=ls
linuxd: image plan failed: Elf(SegmentOutsideUserRange)
process: child exited (2)
linux-root: busybox exit=2
init exited (0)
```

The current `linuxd` image parser rejects BusyBox's PIE segments (`vaddr 0` below the user window)
before it reaches `PT_INTERP`; after B33 adds the PIE bias and interpreter mapping, BusyBox will also
need B25 `fstat`/`newfstatat`/`stat` and the B23 file calls for `ls`. So the qtest **passes on the
recipe and records the boundary**; it does **not** claim that `busybox ls` executes. When B33 lands,
the expectation changes from the `image plan failed` line to the BusyBox `ls` output and status 0;
the fixture and the recipe stay.

The qtest requires the user-supplied tarball and fails with a clear diagnostic when it is absent
(the same policy as the `musl-gcc` fixtures: no silent skip).

## 6. Why the run is partial

The ticket's exit criterion is `/compat/linux/bin/busybox ls` runs. It cannot run on this
`origin/master`:

* B33 (dynamic loading: `PT_INTERP`, `AT_BASE`, PIE bias) is blocked **by this ticket** and is not
  merged; the interpreter file is staged but linuxd cannot map it.
* B25 (`fstat`, `newfstatat`, `stat`) and B23 (file calls) are not merged; BusyBox `ls` needs
  them after the loader starts.
* B32's Owns allows only the add-on, qtest and documentation; the PIE bias/interpreter work belongs
  to B33, and editing linuxd here is explicitly forbidden.

This is a sequencing limitation, not a missing recipe artifact: `--stage` and `--qtest` produce the
tree and image, and the qtest records the exact next failure. The recipe is ready for B33 to
consume.

## 7. Provenance

* Recipe shell script, fixture Rust, qtest TOML and this document: first-party BSD-3-Clause.
* The Alpine minirootfs is a user-supplied binary collection under its packages' own licences
  (BusyBox GPL-2.0-only). It is not committed, not redistributed, and not linked into the product.
* No donor source was copied. The `linux-raw-sys` donor was not needed; the recipe's pinned values
  come from the Alpine release index and the tarball's `.sha256` file.
