<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Linux root recipe (B32)

This add-on stages an **Alpine minirootfs** as the Linux root tree at
`build/linux-root/compat/linux` and builds the `linux-root` qtest initfs. It is a recipe, not a
product binary: the tarball is supplied by the user and never committed.

## Pinned source

| | |
|---|---|
| Source | Alpine Linux minirootfs |
| Version | 3.24.4 |
| Architecture | x86_64 |
| URL | https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/x86_64/alpine-minirootfs-3.24.4-x86_64.tar.gz |
| SHA-256 | `cdaa38cb8de3537967ee5b6c9b6cd00cc962da2b018e6eea48696bb274923012` |
| Size | 3,704,088 bytes |
| Licence | The minirootfs is a collection of Alpine packages, each with its own licence (see `/lib/apk/db/installed` in the tree). BusyBox is GPL-2.0-only. The recipe and the qtest fixture are BSD-3-Clause. |

The hash is also in `alpine-minirootfs-3.24.4-x86_64.tar.gz.sha256` and `recipe.env`. The tarball
and the staged tree live under `build/` and are ignored by git.

## Usage

```sh
sh addons/linux-root/build.sh --check    # verify the pinned recipe; no tarball needed
sh addons/linux-root/build.sh --fetch    # optional: download the pinned tarball (network)
sh addons/linux-root/build.sh --stage    # verify the tarball and stage build/linux-root/compat/linux
sh addons/linux-root/build.sh --qtest    # stage and build build/linux-root/initfs.img
```

`--check` validates the recipe metadata, the checksum file and (when present) the tarball's hash,
size and members. `--stage` and `--qtest` require the tarball at
`build/linux-root/downloads/alpine-minirootfs-3.24.4-x86_64.tar.gz` or the file named by
`RAVEN_ALPINE_MINIROOTFS`. The qtest fails with a clear message when the user-supplied tarball is
absent; it never downloads during a build or test.

## qtest

`tests/qtest/cases/linux-root.toml` boots a native fixture (`tests/qtest/cases/linux-root-fixture`)
as `/init` with the staged tree packed into the initfs. The fixture checks the release file, the
BusyBox binary and the musl loader, then spawns `linuxd` with the exact
`/compat/linux/bin/busybox ls` argument vector. B32a loads the PIE main at its bias and maps the
musl interpreter at its own base, so BusyBox starts and runs `ls`; `ls` then stops at `stat(2)`,
which is B25 work. The serial output records `ls: .: Function not implemented`, the dispatcher's
`stat` hit and the statuses. See `docs/linux-root.md` for the full status.

## Layout

```
addons/linux-root/
├── build.sh
├── recipe.env
├── alpine-minirootfs-3.24.4-x86_64.tar.gz.sha256
└── README.md
build/linux-root/
├── downloads/alpine-minirootfs-3.24.4-x86_64.tar.gz   (user-supplied, gitignored)
├── compat/linux/...                                   (staged tree, gitignored)
├── manifest.txt                                       (source, hash, release, counts)
└── initfs.img                                         (qtest image)
```

B33 and later can serve the staged tree as a mounted `/compat/linux`; B32a packs the regular files
into the qtest initfs so `linuxd` can load the PIE main and its interpreter today.
