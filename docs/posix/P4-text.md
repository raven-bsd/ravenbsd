# P4c - POSIX text utility behaviour evidence

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

This report covers the text-processing utility rows assigned to P4c. The executable cases live
under `tests/posix/utility/text/<tool>/` and are driven by `tests/posix/utility/run-text.sh`.

## Method

* Each case runs the vendored uutils source from `userland/uutils/src/uu/<tool>` through a host
  build. The in-image binaries are linked for `x86_64-unknown-ravenbsd` and need the Raven kernel;
  the host build exercises the same utility source, argument parsing, locale-sensitive comparison,
  standard-input/output and exit-status paths. The Raven target runtime is not claimed by these
  cases.
* The runner builds the selected packages once into `build/posix-utility/uutils-host` and runs each
  `test.sh` in a fresh temporary directory with `LC_ALL=C`.
* Cases use only the utility under test plus the test harness (`sh`, `cmp`, `od`, `sed`, `tail`,
  `seq`) on the host. No utility binary was edited and no shared uutils source was changed.
* Every case prints one `ok:` line per assertion and exits nonzero on the first failed assertion.

```sh
sh tests/posix/utility/run-text.sh
sh tests/posix/utility/run-text.sh --list
sh tests/posix/utility/run-text.sh sort cut tr
```

A present tool with a failing case fails the run. A missing implementation is printed as a
`DEFECT` row and does not claim a pass; the runner still exits zero when every present tool passes.
The defects below are the reason the text family is partial, not present.

## Tool matrix

| Tool | Cases | Result | Evidence |
|---|---:|---|---|
| `sort` | 11 | pass | `tests/posix/utility/text/sort/test.sh` |
| `cut` | 9 | pass | `tests/posix/utility/text/cut/test.sh` |
| `tr` | 8 | pass | `tests/posix/utility/text/tr/test.sh` |
| `wc` | 8 | pass | `tests/posix/utility/text/wc/test.sh` |
| `head` | 8 | pass | `tests/posix/utility/text/head/test.sh` |
| `tail` | 8 | pass | `tests/posix/utility/text/tail/test.sh` |
| `uniq` | 7 | pass | `tests/posix/utility/text/uniq/test.sh` |
| `comm` | 6 | pass | `tests/posix/utility/text/comm/test.sh` |
| `fold` | 5 | pass | `tests/posix/utility/text/fold/test.sh` |
| `expand` | 6 | pass | `tests/posix/utility/text/expand/test.sh` |
| `unexpand` | 6 | pass | `tests/posix/utility/text/unexpand/test.sh` |
| `paste` | 6 | pass | `tests/posix/utility/text/paste/test.sh` |
| `join` | 7 | pass | `tests/posix/utility/text/join/test.sh` |
| `nl` | 6 | pass | `tests/posix/utility/text/nl/test.sh` |
| `pr` | 4 | pass | `tests/posix/utility/text/pr/test.sh` |
| `od` | 7 | pass | `tests/posix/utility/text/od/test.sh` |
| `split` | 8 | pass | `tests/posix/utility/text/split/test.sh` |
| `csplit` | 9 | pass | `tests/posix/utility/text/csplit/test.sh` |
| `tsort` | 6 | pass | `tests/posix/utility/text/tsort/test.sh` |
| `tee` | 8 | pass | `tests/posix/utility/text/tee/test.sh` |
| `printf` | 10 | pass | `tests/posix/utility/text/printf/test.sh` |
| `basename` | 6 | pass | `tests/posix/utility/text/basename/test.sh` |
| `dirname` | 5 | pass | `tests/posix/utility/text/dirname/test.sh` |
| `expr` | 10 | pass | `tests/posix/utility/text/expr/test.sh` |

Coverage includes standard options, file/stdin selection, multi-file headers and totals, exit
status on success and error, C-locale ordering and character classes, byte/character lists, output
files, suffix/prefix controls, topological ordering, format reuse, and NUL-preserving paths for
`cut`, `tr`, `wc`, `head`, `tail`, `tee`, `printf` and `od`.

## Defects (partial rows)

These tools have no implementation under `userland/uutils/src/uu`, `userland/bin` or the shell
profile. They cannot be tested without writing product code, which P4c does not own.

| Defect | Row | Tool | Impact |
|---|---|---|---|
| `P4c-D1` | `util.awk` | `awk` | no implementation; highest-risk text row remains partial |
| `P4c-D2` | `util.sed` | `sed` | no implementation; highest-risk text row remains partial |
| `P4c-D3` | `util.grep` | `grep` | no implementation; highest-risk text row remains partial |
| `P4c-D4` | `util.cmp` | `cmp` | no implementation; binary comparison row remains partial |
| `P4c-D5` | `util.diff` | `diff` | no implementation; text/binary difference row remains partial |
| `P4c-D6` | `util.strings` | `strings` | no implementation; printable-string extraction row remains partial |

## Evidence overlay

The ready overlay rows use structured `test:` evidence and the per-tool `test.sh` paths above.
They cannot be added to `docs/posix/evidence/P4-text.toml` until the P0b overlay loader accepts
owner subfiles: `tools/posix-conform/src/overlay.rs` currently accepts only exact file stems
`P1.toml` through `P5.toml`, and rejects `P4-text.toml` with:

```
overlay file `.../docs/posix/evidence/P4-text.toml` must be named P1.toml through P5.toml
```

That is a P0b dependency defect (`P4c-D7`). The rows are otherwise validated: each `test:` path
exists, has a `#!/bin/sh` shebang and passes under the reworked `test:` runner semantics. Until the
loader accepts the owner subfile, `posix-check` cannot mark the text rows present and the text
family remains partial.
