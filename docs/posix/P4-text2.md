<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# POSIX text tools: grep, cmp, diff, strings, sed, awk (P4e/P4h/P4i)

Status: **DONE for the six implemented tools.** Host behaviour tests pass for `grep`, `cmp`,
`diff`, `strings`, the bounded `sed` subset and the bounded `awk` core; the evidence overlay
registers all six rows. No text utility from the original P4c defect list remains missing.

## Method

Each tool is a first-party Rust crate under `userland/bin/` (`raven-grep`, `raven-cmp`,
`raven-diff`, `raven-strings`, `raven-sed`, `raven-awk`), `no_std` on the target and
`#![forbid(unsafe_code)]`. The host tests exercise the same pure logic under `LC_ALL=C` through
`cargo nextest`; the target binaries need the Raven kernel and are not claimed as board evidence.

```sh
sh tests/posix/utility/run-text2.sh
sh tests/posix/utility/run-text2.sh --list
sh tests/posix/utility/run-text2.sh grep cmp diff strings sed awk
```

## Tool matrix

| Tool | Options/behaviour | Cases | Evidence |
|---|---|---|---|
| `grep` | `-i -v -n -l -q -c -E -F`, basic/extended/fixed, multiple files, exit 0/1/2 | 7 | `tests/posix/utility/text2/grep/test.sh` |
| `cmp` | `-l -s`, byte comparison, EOF, exit 0/1/2 | 5 | `tests/posix/utility/text2/cmp/test.sh` |
| `diff` | normal and `-u` unified output, add/change/delete, exit 0/1/2 | 5 | `tests/posix/utility/text2/diff/test.sh` |
| `strings` | `-a -n`, printable ASCII runs, multiple files | 5 | `tests/posix/utility/text2/strings/test.sh` |
| `sed` | BRE `s///` (`g`/`p`/numeric), line/`$`/`/re/`/ranges, `-n`, `-e`, `-f`, `p d q = y a i c` | 8 | `tests/posix/utility/text2/sed/test.sh` |
| `awk` | records/fields, BEGIN/END, rules, operators, variables, arrays, if/while/for, print/printf, `-v`/`-f`, `length substr index split sub gsub match sprintf int` | 10 | `tests/posix/utility/text2/awk/test.sh` |

`grep`, `sed` and `awk` use the existing `raven-libc-regex` engine (the libc regex exposed to
userspace); no new external dependency was added.

## Limitations

* The target image does not yet stage these binaries or expose a general standard-input handle;
  the host tests cover the byte-stream core. `grep`/`strings` with no file therefore remain a
  target-shell follow-up, as does the image profile entry.
* `grep` supports the requested flags but not the full POSIX `-e`/`-f` option set.
* `diff` renders one unified hunk with full context rather than minimal hunks; normal and unified
  exit statuses are correct.
* `sed` supports the bounded subset in the matrix; `-E`, `n/N/D/P/G/H/h/x/b/t/:`, multiline
  pattern/hold spaces and the full `-e`/`-f` command grammar remain unsupported and are explicit
  defects.
* `awk` supports the bounded core in the matrix; user functions, `getline`, pipes, `close`,
  `system`, output redirection, full regex escapes and the unlisted built-ins are explicit
  defects. `-F`/`FS` support whitespace and a literal separator, not an arbitrary regex.
* The target image does not yet stage the text2 binaries or expose general stdin; host tests cover
  the core.

## Evidence overlay

`docs/posix/evidence/P4-text2.toml` contains the six structured `test:` rows. The P0b loader
accepts the `P4-text2` batch file; `posix-check` executes each script and reports the rows as
present. No row is present without a passing case.
