<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `stat -f` and the RavenBSD filesystem type (K184)

Status: **OBSERVED at `f663e238`.** `stat -f /` on the release image panicked with
`fsext::fs_type(): not implemented` because Raven's `statvfs` has no `f_type`. The fixed image
reports the defined unknown type as `UNKNOWN (0x0)` and exits zero.

## 1. Pre-fix transcript

The release image was booted and logged in as root:

```text
raven# /bin/stat -f /; echo fs1=$?
  File: /

thread 'main' (1) panicked at src/uucore/src/lib/features/fsext/mod.rs:703:9:
not implemented
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
fs1=134

raven# /bin/stat -f -t '%t' /; echo fs2=$?
stat: cannot read file system information for '%t': No such file or directory
...
fs2=134

raven# /bin/stat -f -T /; echo fs3=$?
error: unexpected argument '-T' found
...
fs3=1
```

`-t` is the uutils terse flag and `-T` is not a uutils option, so the ticket's `-t '%t'` and
`-T` spellings cannot reach the type path. The equivalent format forms are `-c '%t'` and
`-c '%T'`; both also panicked pre-fix with exit 134. The stat-fs qtest uses those valid `-c`
forms so it exercises `fs_type` rather than a CLI usage error.

## 2. Mapping

`FsMeta::fs_type` for `target_os = "ravenbsd"` returns `UNKNOWN_FS_TYPE = 0`. Zero is a defined
unknown, not a donor filesystem magic: `pretty_fstype` has no real filesystem at zero and renders
it as:

```text
UNKNOWN (0x0)
```

`stat -f -c '%t'` prints the same value in hex (`0`). The default `stat -f` line is:

```text
    ID: 0        Namelen: 255     Type: UNKNOWN (0x0)
```

No libc ABI, protocol, syscall or `statvfs` field is added. `df`, `stat` default output and the
existing Linux/BSD/macOS mappings are unchanged. The other platforms that still lack a type keep
their existing `unimplemented!()` path; only ravenbsd gets the defined unknown.

## 3. Regression

`tests/qtest/cases/stat-fs.toml` boots the release image, logs in, and asserts:

- `/bin/stat -f /` exits 0 and prints `Type: UNKNOWN (0x0)`;
- `/bin/stat -f -c '%t' /` exits 0 and prints `0`;
- `/bin/stat -f -c '%T' /` exits 0 and prints `UNKNOWN (0x0)`;
- no `panic`/`unimplemented` line appears.

Observed post-fix:

```text
$ timeout 300 cargo xtask qtest --case stat-fs --arch x86_64
stat-fs: pass                    # duration_ms 185026
  File: /
    ID: 0        Namelen: 255     Type: UNKNOWN (0x0)
Block size: 4096       Fundamental block size: 4096
Blocks: Total: 10374      Free: 0          Available: 0
Inodes: Total: 233        Free: 0
fs1=0
0
fs2=0
UNKNOWN (0x0)
fs3=0
exit 0
```

## 4. Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| Filesystem type identity | Some `statvfs` interfaces carry `f_basetype`/`f_fstypename`; CDDL behaviour only. | FreeBSD/NetBSD `statfs` carries `f_fstypename`; the existing libc is BSD-derived, no code copied. | `statfs` carries an `f_type` magic; GPL outcome only. | Raven `statvfs` has no type; use zero and the stable `UNKNOWN (0x0)` label, never an invented magic. |

No donor code was copied. The clone has no `donors.lock`, so no donor revision is quoted.
