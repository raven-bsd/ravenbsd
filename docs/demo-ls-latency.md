<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Demo `ls` latency (K133)

Status: **OBSERVED at product commit `af0e1a7b`.** The demo-login image timed at
`/home/bo/work/ravenos-workspace/ravenbsd-demos/work/product/build/x86_64/ravenbsd-demo-login.img`
reports kernel `af0e1a7b-dirty`. The demo shell is `rsh`; it has no `time` builtin, so the
transcript was measured with a wall clock around each command.

## 1. Reproduction

Five runs on the unmodified demo image:

| Command | Runs (s) | Mean |
|---|---|---:|
| `ls /` | 0.672 / 0.484 / 0.488 / 0.483 / 0.492 | 0.524 |
| `ls /bin` | 1.258 / 0.967 / 0.976 / 0.968 / 0.950 | 1.024 |

Controls:

| Command | Result |
|---|---|
| `echo k133` (shell builtin, one console line) | 0.057 s |
| `/bin/true` (spawn/loader, no output) | 0.146 s |
| `ls /nonexistent` (startup + error, no readdir) | 0.239 s |
| `ls /bin > /dev/null` | not usable: this `rsh` reports `/dev/null: not a directory` |
| `ls /bin > /tmp/list` | 1.201-1.370 s |
| `cat /tmp/list` (same 132 bytes) | 0.083-0.087 s |
| `cat /tmp/list > /tmp/list2` | 0.097-0.124 s |
| `ls /tmp` (empty directory) | 0.293-0.400 s |

## 2. Classification

The fixed cost is **per directory entry in uutils `ls`**, not spawn, output, ramfs or the console:

* `cat` writes the same 132-byte listing to the console or `/tmp` in ~0.1 s, so the console and
  ramfs write paths are fast.
* `/bin/true` is 0.146 s, so spawn/loader is not the 1-3 s cost.
* `ls /tmp` (empty) is ~0.3 s; `ls /bin` grows with the entry count.
* With instrumentation inside `ls`, `write_listing` was 1-31 ms while `list_with_output` was
  0.8-1.4 s.
* `ls --indicator-style=none /bin` was 91-108 ms; default `ls /bin` was 0.8-1.4 s. The Raven
  profile sets `IndicatorStyle::Slash` by default. `indicator_char` called `classify_file` for
  the slash case; `classify_file` calls `is_executable_file`, which calls `metadata()` and
  therefore a vfsd/initfs `stat` for every regular file. A Scheme stat costs tens of
  milliseconds, so a 22-entry `/bin` accumulates ~0.7-1.2 s.

This is not a kernel, console or VFS lock fault; the stat latency itself is expected for the
current Scheme protocol. The unnecessary stat is the uutils `ls` choice.

## 3. Fix

`userland/uutils/src/uu/ls/src/display.rs`: for `IndicatorStyle::Slash`, resolve the slash from
the cached `PathData::file_type()` directory bit and do not call `classify_file`/
`is_executable_file`. Other indicator styles (`classify`, `file-type`) keep the executable
classifier and its stat.

The output contract is unchanged: directories still print with `d/`, files still have no suffix,
and `--indicator-style=none` remains stat-free.

## 4. Evidence at the fix

On the `shell-test` image built from the fixed source (the ticket allows base-tools when the
generated demo profile is unavailable):

| Command | Runs (s) | Mean |
|---|---|---:|
| `ls /` | 0.728 / 0.525 / 0.526 / 0.543 / 0.519 | 0.568 |
| `ls /bin` | 0.536 / 0.537 / 0.533 / 0.780 / 0.764 | 0.630 |

All ten runs are under one second. `base-tools` passes (73.1 s), and the bounded regression
`tests/qtest/cases/base-tools-ls-latency.toml` passes (43.7 s) with 2 s step bounds (normal plus
margin; no cap was raised).

## 5. Donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| `ls` directory classification | Not applicable; no VFS behaviour change. | FreeBSD `bin/ls/ls.c` and NetBSD `bin/ls/ls.c` are permissive BSD-2/3-Clause; both use the directory entry type for `/` and reserve the executable stat for `-F`/`-p`/`-l` paths. Not copied. | GNU coreutils `ls.c` (GPL) is read-only reference; `print_type_indicator` separates the directory test from `is_colored`/executable checks. | Keep uutils `classify_file` for explicit classify/file-type, but resolve the slash-only indicator from `d_type`/cached file type. No donor code copied. |
| Per-entry stat cost | Not applicable. | Not applicable. | The upstream uutils `classify_file` is MIT and remains the source for the other styles; this ticket adds a first-party Raven branch. | First-party change in the vendored MIT uutils; no provenance entry. |
