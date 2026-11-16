<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# POSIX file utility conformance (P4b)

Status: **PARTIAL — 27 host behaviour suites pass; 23 rows are registered in the P0b overlay and
4 tested tools have no profile row.** The four unregistered tools need a profile/seed follow-up
(`P4-file-D1`). No utility binary or shared uutils source was edited and no row is claimed without
a test.

## Method

The 25 uutils-backed tools are built for the host by `tests/posix/utility/run-file.sh` into
`build/posix-utility/uutils-host`; each `tests/posix/utility/file/<tool>/test.sh` then runs the
same source through a deterministic `LC_ALL=C` temporary directory. `file` and `find` are
first-party Rust crates (`userland/bin/file`, `userland/bin/find`); their test scripts run the
host `cargo nextest` behaviour tests. The Raven target binaries need the Raven kernel, so the host
build is the executable behaviour evidence; no target-board result is claimed.

Each case checks standard options, exit status, stdin/stdout and error cases. A present tool with
a failing case fails the runner; a tool with no implementation is a `DEFECT` row and never a pass.

```sh
sh tests/posix/utility/run-file.sh
sh tests/posix/utility/run-file.sh --list
sh tests/posix/utility/run-file.sh cat cp ls
```

## Tool matrix

| Tool | Cases | Result | Evidence |
|---|---:|---|---|
| `cat` | 5 | pass | `tests/posix/utility/file/cat/test.sh` |
| `chgrp` | 3 | pass | `tests/posix/utility/file/chgrp/test.sh` |
| `chmod` | 5 | pass | `tests/posix/utility/file/chmod/test.sh` |
| `chown` | 4 | pass | `tests/posix/utility/file/chown/test.sh` |
| `cp` | 5 | pass | `tests/posix/utility/file/cp/test.sh` |
| `cksum` | 3 | pass | `tests/posix/utility/file/cksum/test.sh` |
| `dd` | 3 | pass | `tests/posix/utility/file/dd/test.sh` |
| `df` | 3 | pass | `tests/posix/utility/file/df/test.sh` |
| `du` | 5 | pass | `tests/posix/utility/file/du/test.sh` |
| `file` | 1 runner / 20 Rust | pass | `tests/posix/utility/file/file/test.sh`, `userland/bin/file/tests/host.rs` |
| `find` | 1 runner / 20 Rust | pass | `tests/posix/utility/file/find/test.sh`, `userland/bin/find/tests/host.rs` |
| `link` | 3 | pass | `tests/posix/utility/file/link/test.sh` |
| `ln` | 5 | pass | `tests/posix/utility/file/ln/test.sh` |
| `ls` | 7 | pass | `tests/posix/utility/file/ls/test.sh` |
| `mkdir` | 5 | pass | `tests/posix/utility/file/mkdir/test.sh` |
| `mkfifo` | 3 | pass | `tests/posix/utility/file/mkfifo/test.sh` |
| `mknod` | 3 | pass / no profile row | `tests/posix/utility/file/mknod/test.sh` |
| `mv` | 5 | pass | `tests/posix/utility/file/mv/test.sh` |
| `pathchk` | 3 | pass | `tests/posix/utility/file/pathchk/test.sh` |
| `pwd` | 3 | pass | `tests/posix/utility/file/pwd/test.sh` |
| `readlink` | 3 | pass / no profile row | `tests/posix/utility/file/readlink/test.sh` |
| `realpath` | 3 | pass / no profile row | `tests/posix/utility/file/realpath/test.sh` |
| `rm` | 4 | pass | `tests/posix/utility/file/rm/test.sh` |
| `rmdir` | 5 | pass | `tests/posix/utility/file/rmdir/test.sh` |
| `stat` | 3 | pass / no profile row | `tests/posix/utility/file/stat/test.sh` |
| `touch` | 5 | pass | `tests/posix/utility/file/touch/test.sh` |
| `unlink` | 3 | pass | `tests/posix/utility/file/unlink/test.sh` |

Total: 27 tested tools, 101 runner cases plus the 20 Rust `file`/`find` tests, 0 failing present
tool. The P0b overlay registers 23 of them (`file` and `find` included); `mknod`, `readlink`,
`realpath` and `stat` have no profile row, so their tests cannot be registered without a
profile/seed change.

## P4b2 implementation

* `userland/bin/file`: first-party `raven-file` crate. Supports `-b` (brief), `-i` (MIME), `--`,
  exit status 0/1/2, text/binary/ELF/directory/symlink/FIFO/device/socket detection. `-m` is
  optional in the ticket and is not implemented; an unknown option is a usage error.
* `userland/bin/find`: first-party `raven-find` crate. Supports paths (default `.`), `-name`
  with `*`/`?`, `-type f|d|l`, `-perm mode`, `-print`, correct non-zero exit on a missing start,
  and does not follow symlinks (so no symlink loops). `-exec` is optional and is not implemented.
* Host tests cover real text/binary/ELF/directory/symlink classification and a real directory
  tree with name/type/perm filters, a symlink and a symlink loop.

## Defects

| Defect | Row/tool | Impact |
|---|---|---|
| `P4-file-D1` | `util.mknod`, `util.readlink`, `util.realpath`, `util.stat` | no profile rows exist; their 4 passing tests cannot be registered from P4b Owns |

`file` and `find` are now present rows. The four profile-less tools need a seed/profile follow-up;
they are not waived and not counted as present in the gate.

## Evidence overlay

`docs/posix/evidence/P4-file.toml` is committed with 23 structured `test:` rows for the profile
rows that exist, including `util.file` and `util.find`. The P0b loader accepts the owner batch
file and the gate executes each row. The four tools without profile rows are deliberately absent
from the overlay so they cannot create unsupported-claim failures.

```toml
schema = 1
standard = "posix-2024"

[[row]]
id = "util.cat"
status = "present"
evidence = "test:tests/posix/utility/file/cat/test.sh"
notes = "P4b: file-family behaviour test"
```

The gate run reports those 23 as pass; the file family has no remaining profile-row failure.
The four profile-less tools remain outside the gate until the profile is extended.

## Qtest scope

No `tests/qtest/cases/posix-util-file*` case was added. The uutils family is host-tested against
the same source and `file`/`find` are first-party host-tested; a qtest would boot the target for
the same code and duplicate the coverage. The target binaries and image staging remain unclaimed
by this ticket; the profile does not yet stage `raven-file`/`raven-find`.
