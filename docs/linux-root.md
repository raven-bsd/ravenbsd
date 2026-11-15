<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Linux root recipe and PIE interpreter path (B32/B32a)

Status: B32a, 2026-11-15. Evidence tags: **OBSERVED** (run on the target or read from the source),
**PROPOSED** (not implemented), **UNVERIFIED** (not exercised).

## 1. What this adds

`linuxd` runs static images from the initfs; the Linux corpus needs a real root tree. B32 added the
add-on recipe `addons/linux-root/`, which stages an Alpine minirootfs at
`build/linux-root/compat/linux` and builds the `linux-root` qtest. B32a completes the loader path
for that root: a `ET_DYN` main is parsed with a zero-based window and loaded at
`Config::main_pie_base`; its `PT_INTERP` image is read from the root, loaded at
`Config::interp_base`, and the initial thread starts at the interpreter entry with `AT_BASE` and
`AT_ENTRY` from `linux-elf`'s plan. BusyBox now starts and runs `ls`; `ls` stops at the B25
`stat(2)` boundary.

The tarball is user-supplied and never committed; the recipe never downloads during a build or a
test (`--fetch` is the only network operation and is user-invoked).

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

The full tree is staged on disk for B33 and later. The qtest initfs packs every regular file of
the tree under `/compat/linux/...`; the Alpine symlinks are not packed because the initfs format
has no symlink entries. The exact image is `/init` (the fixture), `/sbin/linuxd`, and the staged
tree. The invocation is `/bin/busybox ls` (multicall with an applet argument), so the symlinks are
not needed for this test.

The recipe does **not** copy any code from Alpine; it extracts a user-supplied binary rootfs. The
tree and tarball are outside git (`addons/linux-root/.gitignore` ignores `downloads/`; the root
`build/` is already ignored).

## 5. Loader path (B32a)

`linuxd::image`:

* reads `e_type`; an `ET_DYN` main is parsed with `0..USER_END` because its raw segments start at
  zero, and `ET_EXEC` keeps `USER_MIN..USER_END`;
* reads `PT_INTERP` with `interp_path`, resolves the absolute path against the Linux root
  (`/compat/linux` prefix; the literal initfs path is tried first) and parses the interpreter as
  `ET_DYN`;
* calls `linux-elf::plan_image` with the interpreter, so `AT_PHDR`, `AT_PHENT`, `AT_PHNUM`,
  `AT_BASE`, `AT_ENTRY` and the initial PC are computed by the one owner of those rules;
* collects raw steps for both images, checks that every biased step stays in the user window, and
  hands the main bias/interpreter bias to the loader.

`linuxd::loader` maps the main steps at `spec.bias` and the interpreter steps at
`spec.interp_bias`, records both sets of regions for copy-in/out, and starts the first thread at the
interpreter entry. The `brk` arena is now mapped at the **end of the main image** (`spec.brk_start`)
instead of a fixed address; musl's `brk` growth writes into mapped memory, and the handler's
`brk_start`/`brk_limit` agree with the page tables. The initial stack's `argv` now uses the full
argument vector linuxd received, so the fixture's `ls` reaches BusyBox (it previously planned only
`argv[0]`).

`linuxd` propagates the Linux process's exit status as its own; a supervisor such as the
`linux-root` fixture can print the status without parsing the console.

## 6. qtest and observed boundary

`tests/qtest/cases/linux-root.toml` boots the native fixture
`tests/qtest/cases/linux-root-fixture` as `/init`. The fixture:

1. parses the initfs and reads `/compat/linux/etc/alpine-release`,
   `/compat/linux/bin/busybox` and `/compat/linux/lib/ld-musl-x86_64.so.1`, printing their sizes;
2. spawns `/sbin/linuxd` with the exact argument vector
   `["/sbin/linuxd", "/compat/linux/bin/busybox", "ls"]`, so linuxd sees the Linux
   `argv = ["/compat/linux/bin/busybox", "ls"]`;
3. waits and records linuxd's exit status.

**OBSERVED** serial output (`build/qtest/linux-root.log`):

```
linux-root: release=3.24.4
linux-root: busybox bytes=804616 loader bytes=670312
linux-root: attempt path=/compat/linux/bin/busybox argv1=ls
linuxd: entry=0x18000006c185 phdr=0x100000000040 phnum=12 brk=0x1000000c6000
linuxd: dynamic=1 interp-entry=0x18000006c185 interp-bias=0x180000000000
linuxd: mapped 369 regions, sp=0x7fffffffee60
ls: .: Function not implemented
linuxd: unimplemented nr=4 name=stat hits=1
linuxd: hello exit=1
linux-root: linuxd exit=1
init exited (0)
```

The main PIE is based at `0x1000_0000_0000`, the musl loader at `0x1800_0000_0000`; the initial
PC is `0x1800_0006_c185` and `AT_ENTRY` is the PIE's `0x1000_0006_ed1`. BusyBox runs, `ls` calls
`stat(2)` (x86-64 nr 4), the dispatcher has no handler and returns `-ENOSYS`, and BusyBox prints
`Function not implemented` and exits 1. The qtest asserts the `stat` hit and the status; it is the
exact B25 boundary.

The qtest requires the user-supplied tarball and fails with a clear diagnostic when it is absent
(the same policy as the `musl-gcc` fixtures: no silent skip).

## 7. Remaining boundary

`/compat/linux/bin/busybox ls` starts but does not list: B25 (`stat`, `newfstatat`, `fstat`) is not
merged. After B25, `ls` will also need the B23 file calls and possibly the B22 file-backed `mmap`
for other applets. B33 (the wider dynamic-loading ticket) remains the place for `dlopen`, the full
root filesystem service and the remaining loader features; this ticket only makes the root's
BusyBox start from the initfs.

## 8. Provenance

* Recipe shell script, fixture Rust, qtest TOML and this document: first-party BSD-3-Clause.
* The Alpine minirootfs is a user-supplied binary collection under its packages' own licences
  (BusyBox GPL-2.0-only). It is not committed, not redistributed, and not linked into the product.
* No donor source was copied. The `linux-raw-sys` donor was not needed; the recipe's pinned values
  come from the Alpine release index and the tarball's `.sha256` file.
