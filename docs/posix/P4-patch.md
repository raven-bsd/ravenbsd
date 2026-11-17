<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# POSIX patch utility (P4m)

Status: **DONE for the bounded unified-diff core.** `raven-patch` applies strict unified diffs
with `-i`, `-pN`, `-R`, `-u` and `-f`, creates and deletes files, and rejects any hunk whose
context does not match. `util.patch` is registered as present only through the host behaviour
script.

## Method

`userland/bin/patch` is a first-party Rust crate (`raven-patch`), `no_std` on the target and
`#![forbid(unsafe_code)]`. The target binary uses `raven-rt`, `raven-tool-core` and
`raven-proto-fs`; the host binary uses the standard library for file I/O and standard input.

`tests/posix/utility/patch/test.sh` is self-contained: it builds the host `raven-patch` binary and
the `gen-diff` example, generates forward/reverse diffs with the first-party `raven-diff` engine,
applies them, and compares the resulting files. It also exercises stdin, multiple patch operands,
multiple file diffs in one patch, `-pN`, create/delete, and rejection of bad hunks.

```sh
sh tests/posix/utility/run-patch.sh
sh tests/posix/utility/patch/test.sh
```

## Behaviour matrix

| Area | Implemented | Cases |
|---|---|---|
| Input and direction | `-i patchfile`, patch-file operands, standard input, multiple operands, multiple file diffs, `-R` | 7 |
| Paths | `-pN` and `-p N` strip leading components; `..` components are rejected | 1 |
| Files | modify, create (`--- /dev/null` or a zero old range), delete (`+++ /dev/null`) | 3 |
| Options | `-u` and `-f` accepted; matching is still strict | 1 |
| Matching | exact context/removed-line match at the expected offset; no fuzz; atomic rejection | 3 |
| Diagnostics | missing target and unknown option | 1 |

A hunk that does not match rejects the entire invocation before any file is written, so a later
bad hunk cannot leave an earlier hunk partially applied.

## Known limits and defects

* Unified diffs only. Context diffs, ed scripts and binary patches are unsupported.
* Context fuzz is not implemented; an offset hunk is rejected rather than searched for. This is an
  explicit defect, not a silent misapplication.
* `-f` is accepted for command-line compatibility but does not relax matching.
* `-u` is accepted and is a no-op because unified input is the only supported format.
* Header timestamps are ignored only when separated by a tab; the path itself is used verbatim.
* No backup files, reject files, interactive prompts, file-mode changes, or `-b`/`-V` support.
* The target standard input path reads `tty:console`; there is no general process-stdin handle or
  pipeline plumbing in the image yet. The target binary is not staged in the image, so the host
  suite is the executable evidence.

## Evidence overlay

`docs/posix/evidence/P4-patch.toml` contains the `util.patch` row with
`test:tests/posix/utility/patch/test.sh`. The P0b loader accepts the `P4-patch` owner batch file
and the gate executes the script; the row is present only while every case passes.
