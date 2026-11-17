<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4w - bounded POSIX make

The first-party utility lives in `userland/bin/make/`. The host entry point runs recipes through
`/bin/sh -c`; the target entry point spawns the first word of a recipe from `/bin` or `/sbin`
and rejects shell metacharacters (redirection, pipes, subshells) explicitly. The parser and
planner are `no_std` + `alloc` and shared by both.

## Supported matrix

| Feature | Status | Evidence |
|---|---|---|
| default goal | supported | `test.sh` `case_default_goal`; `lib_ut` |
| explicit rules | supported | `lib_ut`; all host cases |
| variable `=`, `:=`, `?=`, `+=` | supported | `lib_ut`; `case_variables` |
| `$@`, `$<`, `$^`, `$?` | supported | `lib_ut`; `case_automatic` |
| pattern rules `%` | supported | `lib_ut`; `case_pattern` |
| suffix rules `.c.o` | supported | `lib_ut`; `case_suffix` |
| `.PHONY` | supported | `lib_ut`; `case_phony` |
| `include` / `-include` | supported | `lib_ut`; `case_include` |
| `-f`, `-n`, `-s`, `-k`, `-j1` | supported | `lib_ut`; `test.sh` |
| command failure propagation | supported | `lib_ut`; `case_failure` |
| recipe `@` / `-` prefixes | supported | `lib_ut` |
| GNU functions (`wildcard`, `shell`, `patsubst`, ...) | unsupported | documented usage error / empty expansion |
| parallel jobs `-jN`, N > 1 | explicit unsupported error | `parse_args`; `case_unsupported_parallel` |
| shell metacharacters on the target runner | explicit error | target `SpawnRunner`; host tests use `/bin/sh` |

The target runner intentionally remains bounded: a recipe must be a simple program invocation
whose executable is in `/bin` or `/sbin`; use a script for redirection or pipelines.

## Acceptance

```
sh tests/posix/utility/run-make.sh
make: ok

cargo nextest run -p make
11 passed; 0 failed

cargo xtask posix-check --profile posix --arch x86_64
util.make pass (test:tests/posix/utility/make/test.sh)

make check-fast
check-fast: PASSED
```

The evidence overlay is `docs/posix/evidence/P4-make.toml`.
