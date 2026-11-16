<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4f/P4g - remaining vendored uutils behaviour evidence

This report covers the remaining vendored uutils tools that P4b/P4c/P4d do not own. P4f added the
behaviour cases; P4g registered the 35 adjacent tools in the POSIX profile. The runner is
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
* No vendored uutils source or product utility binary was edited.

```sh
sh tests/posix/utility/run-misc.sh
sh tests/posix/utility/run-misc.sh --list
sh tests/posix/utility/run-misc.sh base64 seq timeout
```

## Tool matrix

All 40 tools in this batch have a passing case and a `util.*` row:

| Tool | Cases | Row | Tool | Cases | Row |
|---|---:|---|---|---:|---|
| `arch` | 3 | `util.arch` | `md5sum` | 3 | `util.md5sum` |
| `b2sum` | 4 | `util.b2sum` | `mktemp` | 4 | `util.mktemp` |
| `base32` | 4 | `util.base32` | `numfmt` | 5 | `util.numfmt` |
| `base64` | 4 | `util.base64` | `nproc` | 3 | `util.nproc` |
| `basenc` | 5 | `util.basenc` | `pinky` | 3 | `util.pinky` |
| `dircolors` | 4 | `util.dircolors` | `printenv` | 4 | `util.printenv` |
| `factor` | 4 | `util.factor` | `ptx` | 4 | `util.ptx` |
| `false` | 2 | `util.false` | `seq` | 6 | `util.seq` |
| `fmt` | 3 | `util.fmt` | `sha1sum` | 3 | `util.sha1sum` |
| `groups` | 4 | `util.groups` | `sha224sum` | 3 | `util.sha224sum` |
| `hostid` | 3 | `util.hostid` | `sha256sum` | 3 | `util.sha256sum` |
| `install` | 4 | `util.install` | `sha384sum` | 3 | `util.sha384sum` |
| `logname` | 3 | `util.logname` | `sha512sum` | 3 | `util.sha512sum` |
| `sleep` | 4 | `util.sleep` | `shred` | 4 | `util.shred` |
| `stdbuf` | 4 | `util.stdbuf` | `shuf` | 5 | `util.shuf` |
| `sum` | 4 | `util.sum` | `sync` | 3 | `util.sync` |
| `tac` | 4 | `util.tac` | `timeout` | 4 | `util.timeout` |
| `true` | 3 | `util.true` | `truncate` | 6 | `util.truncate` |
| `uptime` | 4 | `util.uptime` | `users` | 3 | `util.users` |
| `who` | 4 | `util.who` | `yes` | 3 | `util.yes` |

`link` is deliberately not duplicated: P4b already owns `util.link` with a passing row.

## Evidence overlay

`docs/posix/evidence/P4-misc.toml` now contains 40 present rows, each pointing at
`test:tests/posix/utility/misc/<tool>/test.sh`. `P4g` added the 35 tool names to
`tools/posix-conform/src/seed.rs` `UTILITIES` (216 names total) and regenerated
`docs/posix/profile.toml` and `docs/posix/traceability.toml` from the seed. No fake row or waiver
is present.

## Gate result

```
posix-check: 855 pass, 942 fail, 11 skip; FAIL (release blocked)
utility area: 95 pass, 121 fail, 0 skip
```

Before P4f/P4g the utility area was 51 pass / 130 fail, and after P4f's five rows it was
56 pass / 125 fail; the whole gate was 816 pass / 946 fail, 11 skip. P4g adds 35 rows and the seed
regeneration records four existing P4e `grep`/`cmp`/`diff`/`strings` claims, so the measured delta
is +39 utility passes and -4 utility failures; the whole gate is 855/942/11. Every new row ran its
P4f case in the gate.

## Defects

No P4f/P4g defect remains. `P4f-D1` (35 tools without profile rows) is resolved by P4g.

## Limitations

* Host build of the vendored source, not the Raven target runtime; no board claim.
* `groups`, `logname`, `users`, `who`, `uptime` and `pinky` use the host account/utmp state for
  their positive assertions. They are still real behaviour checks, but the output content is
  environment-dependent; the cases assert status and stable format markers.
* `sleep`/`timeout` use short intervals (0.01-0.1 s) to keep the gate quick.
