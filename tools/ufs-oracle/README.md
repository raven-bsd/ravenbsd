<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# UFS oracle tools

Host-side oracles for the UFS work (B63, DR-23): NetBSD `makefs` to create
UFS1/UFS2 images and `fsck_ffs` to judge them. Nothing here is linked into
RavenBSD; the tools are built in `$UFS_ORACLE_DIR` (default
`/tmp/opencode/ufs-oracle`) from the read-only donor checkout.

OBSERVED 2026-11-10 on Ubuntu 22.04, x86-64, gcc 11.4, GNU make 4.3,
Python 3.10: `build-oracles.sh --force` finishes in 8 s and produces a
working `makefs`. `fsck_ffs` is **not built** (see below).

## Quick start

```sh
sh tools/ufs-oracle/build-oracles.sh            # makefs, 8 s
sh tests/ufs-vectors/make-vectors.sh            # four golden images + manifests
python3 tools/ufs-oracle/audit.py \
    --donor /home/bo/work/ravenos-workspace/ravenbsd-resources/netbsd \
    --out tests/ufs-vectors                     # audit tables
```

`build-oracles.sh` honours `NETBSD_DONOR`, `UFS_ORACLE_DIR`, `CC` and
`CFLAGS`. It rebuilds when the donor revision in `donors.lock` changes; use
`--force` to rebuild unconditionally.

## What the build does

1. Copies `tools/compat` from the donor into `$UFS_ORACLE_DIR/src/tools/compat`
   and runs its autoconf configure script. The generated `nbtool_config.h`
   and `libnbcompat` objects are what make the NetBSD tool sources compile
   against glibc.
2. Compiles the FFS-capable `makefs` front end and back end:
   `usr.sbin/makefs/{makefs,walk,ffs}.c`, `usr.sbin/makefs/ffs/*.c`,
   `sys/ufs/ffs/{ffs_bswap,ffs_subr,ffs_tables}.c`, the mtree helpers
   (`getid.c`, `misc.c`, `spec.c`) and `sbin/mknod/pack_dev.c`.
3. Compiles the libnbcompat pieces those files reference (`efun`, `random`,
   `strlcpy`, `strlcat`, `strsuftoll`, `setprogname`, `pwcache`, `getmode`,
   `vis`, `unvis`, `stat_flags`, `fparseln`, `reallocarr`, `strtoi`,
   `strtou`, `fgetln`, `setpassent`, `setgroupent`).
4. Links a single `makefs` binary. Only the FFS backend is compiled. The
   cd9660, chfs, v7fs, msdos and udf entries in `makefs.c`'s dispatch table
   are filled by a generated stub file, so `-t ffs` (the default) is the
   supported mode and other `-t` values fail cleanly. FFS code itself is
   unmodified NetBSD code.

The generated stub file and all objects live in `$UFS_ORACLE_DIR`; no C
source and no binary is added to this repository.

## Why not `build.sh`

* `sh build.sh -m host ... tools` fails immediately with
  `ERROR: Unknown target MACHINE: host` (OBSERVED, this donor revision).
* `sh build.sh -m amd64 ... tools` would first build `host-mkdep`, `nbmake`,
  then the whole `TOOLCHAIN_BITS` set (`gmake`, `gmp`, `mpfr`, `mpc`,
  `isl`, `binutils`, `gcc`) plus every host tool; that is far outside the
  ticket's 3-hour tool timebox for two binaries.
* The hand-written build above reuses the donor's own
  `tools/compat` configure and the exact source list in
  `usr.sbin/makefs/Makefile` and `usr.sbin/makefs/ffs/Makefile.inc`; it
  needs no NetBSD make, cross compiler or root.

Compilation warnings (mostly `DT_WHT`/`DT_SOCK` redefinitions from glibc's
`<dirent.h>` versus NetBSD's `dir.h`) are harmless; every object's warning
log is kept as `$UFS_ORACLE_DIR/obj/<name>.log`.

## `fsck_ffs`: BLOCKED

`fsck_ffs` is not a host tool in NetBSD's `tools/` set. It is a target
program that uses the NetBSD userland/kernel ABI headers and disk ioctls.
The first compiler error from `sbin/fsck_ffs/main.c` is (OBSERVED):

```
/tmp/opencode/ufs-oracle/nbinclude/ufs/ufs/ufsmount.h:51:31: error: field '_pad1' has incomplete type
   51 |         struct  export_args30 _pad1; /* compat with old userland tools */
```

`struct export_args30` lives in NetBSD's `<sys/mount.h>` and needs
`<sys/ucred.h>`; glibc's `<sys/mount.h>` does not provide it. Further
target-only pieces follow (`sys/disk.h`'s `struct disk_geom`,
`sys/disklabel.h`, `__printflike`, `bswap16/32/64` and the `partutil`
disk-label path). Compiling it on Linux is a port of the checker, not a
build-system fix; it is outside this ticket's tool timebox. The exact log
is kept at `$UFS_ORACLE_DIR/fsck-ffs.log`, and `build-oracles.sh` records
the same blocker on every `--force` run.

Fallbacks (proposed, not implemented here):

1. **NetBSD QEMU guest as an oracle VM**: boot a small NetBSD/amd64 image,
   copy the golden images in and run the real `fsck_ffs -n -F` there.
   Highest fidelity, but needs a guest image and a boot harness.
2. **Linux `ufs` module read-only by the owner with `sudo`**: the module is
   on disk (`/lib/modules/$(uname -r)/kernel/fs/ufs/ufs.ko`, OBSERVED) but
   not loaded. A loop mount with `-o ro` gives a second opinion on the
   read path; it is not a replacement for `fsck_ffs` and its write support
   is doubtful. This needs the owner because it changes the host.

## Licence audit

`audit.py` reads the donor read-only and writes `licence-audit.tsv`,
`LICENCE-AUDIT.md` and `NOTICE-UFS.txt` into `--out`. It is deterministic:
re-running changes no byte. It covers 107 `*.c`/`*.h` files: `sys/ufs/ffs`,
`sys/ufs/ufs`, `sbin/fsck_ffs`, `sbin/newfs`, `usr.sbin/makefs`
(recursive), plus `sys/kern/vfs_wapbl.c` and `sys/sys/wapbl.h`. It
classifies the first 400 lines of each file; it does not recurse into
included headers and is not legal advice.

## Image check

`check-ufs.py <image> <manifest>` is a small independent UFS1/UFS2 reader
for the golden vectors. It reads the superblock at 8192, maps direct,
single, double and triple indirect blocks, walks the tree and compares
every path, type, mode, uid, gid, size, SHA-256, symlink target and link
count with the manifest. `tests/ufs-vectors/make-vectors.sh` runs it on
every image it builds, and OBSERVED all four report
`OK: 1639 paths match`. It is a test oracle, not a mount implementation.

## Provenance and licence

The scripts and the audit are fresh BSD-3-Clause code. No NetBSD source is
copied into the repository by this ticket. `makefs` is built outside the
tree from the donor revision recorded in
`ravenbsd-resources/donors.lock`; its binary is a local oracle, not a
distributed artifact. The FFS source it contains carries the NetBSD
Foundation, Regents of the University of California, Networks Associates
Technology and other notices; the 4-clause Wasabi/UCB files are compiled
only into this local tool and are marked `reference-only` in the audit.
