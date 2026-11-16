<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4f - remaining vendored uutils behaviour evidence

This report covers the remaining vendored uutils tools that P4b/P4c/P4d do not own. The runner is
`tests/posix/utility/run-misc.sh`; each case lives under
`tests/posix/utility/misc/<tool>/test.sh`.

## Method

* Each case runs the vendored uutils source through a host build. The in-image binaries are linked
  for `x86_64-unknown-ravenbsd` and need the Raven kernel; the host build exercises the same
  argument parsing, stdin/stdout, exit-status and file behaviour under `LC_ALL=C`.
* The runner builds the selected `uu_*` packages once into `build/posix-utility/misc-host` and runs
  every script in a fresh temporary directory.
* The harness uses only `sh`, `cmp`, `od`, `grep`, `sort`, `wc`, `head`, `stat`, `id` and `cut` as
  test scaffolding; the tool under test is always the vendored uutils host binary.
* No vendored uutils source, profile row or product utility binary was edited.

```sh
sh tests/posix/utility/run-misc.sh
sh tests/posix/utility/run-misc.sh --list
sh tests/posix/utility/run-misc.sh base64 seq timeout
```

## Tool matrix

All 40 tools in this batch have a passing case:

| Tool | Cases | Result | POSIX row |
|---|---:|---|---|
| `arch` | 3 | pass | no profile row |
| `b2sum` | 4 | pass | no profile row |
| `base32` | 4 | pass | no profile row |
| `base64` | 4 | pass | no profile row |
| `basenc` | 5 | pass | no profile row |
| `dircolors` | 4 | pass | no profile row |
| `factor` | 4 | pass | no profile row |
| `false` | 2 | pass | **`util.false`** |
| `fmt` | 3 | pass | no profile row |
| `groups` | 4 | pass | no profile row |
| `hostid` | 3 | pass | no profile row |
| `install` | 4 | pass | no profile row |
| `logname` | 3 | pass | **`util.logname`** |
| `md5sum` | 3 | pass | no profile row |
| `mktemp` | 4 | pass | no profile row |
| `numfmt` | 5 | pass | no profile row |
| `nproc` | 3 | pass | no profile row |
| `pinky` | 3 | pass | no profile row |
| `printenv` | 4 | pass | no profile row |
| `ptx` | 4 | pass | no profile row |
| `seq` | 6 | pass | no profile row |
| `sha1sum` | 3 | pass | no profile row |
| `sha224sum` | 3 | pass | no profile row |
| `sha256sum` | 3 | pass | no profile row |
| `sha384sum` | 3 | pass | no profile row |
| `sha512sum` | 3 | pass | no profile row |
| `shred` | 4 | pass | no profile row |
| `shuf` | 5 | pass | no profile row |
| `sleep` | 4 | pass | **`util.sleep`** |
| `stdbuf` | 4 | pass | no profile row |
| `sum` | 4 | pass | no profile row |
| `sync` | 3 | pass | no profile row |
| `tac` | 4 | pass | no profile row |
| `timeout` | 4 | pass | no profile row |
| `true` | 3 | pass | **`util.true`** |
| `truncate` | 6 | pass | no profile row |
| `uptime` | 4 | pass | no profile row |
| `users` | 3 | pass | no profile row |
| `who` | 4 | pass | **`util.who`** |
| `yes` | 3 | pass | no profile row |

`link` is deliberately not duplicated: P4b already owns `util.link` with a passing row.

## Evidence overlay

`docs/posix/evidence/P4-misc.toml` registers the five rows that exist in `docs/posix/profile.toml`:

* `util.false` -> `test:tests/posix/utility/misc/false/test.sh`
* `util.logname` -> `test:tests/posix/utility/misc/logname/test.sh`
* `util.sleep` -> `test:tests/posix/utility/misc/sleep/test.sh`
* `util.true` -> `test:tests/posix/utility/misc/true/test.sh`
* `util.who` -> `test:tests/posix/utility/misc/who/test.sh`

The other 35 tools have passing behaviour cases but no `util.*` row in the current POSIX profile
(the profile is the POSIX.1-2024 inventory, not the full vendored uutils set). P4f does not own
`docs/posix/profile.toml` or the seed, so it cannot invent rows for them. They are adjacent
coverage, not claimed present rows.

## Gate result

```
posix-check: 816 pass, 946 fail, 11 skip; FAIL (release blocked)
utility area: 56 pass, 125 fail, 0 skip
```

Before this batch the utility area was 51 pass / 130 fail (the ticket's "130 failing util.* rows").
The five registered rows move it to 56 pass / 125 fail; the whole gate moves from 811 pass / 950
fail to 816 pass / 946 fail.

## Defects

No P4f-D defect: every tool in the batch has a passing host case and no implementation is missing.
The missing profile rows are recorded above as adjacent coverage rather than a tool defect.

## Limitations

* Host build of the vendored source, not the Raven target runtime; no board claim.
* `groups`, `logname`, `users`, `who`, `uptime` and `pinky` use the host account/utmp state for
  their positive assertions. They are still real behaviour checks, but the output content is
  environment-dependent; the cases assert status and stable format markers.
* `sleep`/`timeout` use short intervals (0.01-0.1 s) to keep the gate quick.
