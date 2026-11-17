<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4a/P4p POSIX shell semantics (P4-shell)

Status: P4a/P4p implementation. The host `brush --posix` conformance transcript matches GNU
bash 5.1.16 `--posix` for every marker in `tests/fixtures/posix-shell/posix-shell.sh` except
`$$`. The target `posix-shell` qtest passes 48 markers: the 18 brush rows plus the rsh
pipeline, redirection, list and here-document group. The P0b overlay is merged;
`docs/posix/evidence/P4-shell.toml` registers 22 present rows (18 brush, 4 rsh) against
`qtest:posix-shell` and 3 genuinely missing behaviours as written waivers. The gate executes
the qtest for each present row.

## Evidence

```sh
# Host: the full fixture, one marker per row family.
userland/brush/target/debug/brush --posix tests/fixtures/posix-shell/posix-shell.sh

# Target: the four brush groups plus the rsh pipeline/redirection/here-doc/list group.
cargo xtask qtest --case posix-shell --arch x86_64

# Gate (executes docs/posix/evidence/P4-shell.toml):
cargo xtask posix-check --profile posix --arch x86_64
```

The fixture is first-party and uses only shell builtins. `cargo nextest run -p brush-core`
covers the shell engine's host unit tests. No donor shell source was read or copied.

## Row matrix

| `shell.*` row | Status | Evidence or waiver |
|---|---|---|
| `arithmetic-expansion` | present | qtest `arith:5`; host fixture `arith2` |
| `command-substitution` | present | qtest `sub:inner` |
| `compound-command` | present | qtest `group:ok`, `subshell:ok` |
| `conditional` | present | qtest `if:ok`, `case:ok` |
| `exit-status` | present | qtest `status:1`, `func-status:3`, `errexit:1`, `nounset:1` |
| `field-splitting` | present | qtest `split:3:a:b:c` |
| `function` | present | qtest `func:arg`; host fixture recursive function |
| `getopts` | present | qtest `getopt:a:`, `getopt:b:val`, `optind:4` |
| `here-document` | present | qtest `rsh-here`: `cat <<EOF` in the rsh group; host fixture also proves `<<-`. |
| `interactive-shell` | waived | P9c; the interactive login is the `modern-shell` profile, not this host gate. |
| `job-control` | waived | PG-56 (kernel process groups/sessions/tty control). |
| `list` | present | qtest `rshone`, `rshtwo`, `and:ok`, `or:ok`; asynchronous `$!`/`wait pid` stay in the `job-control` waiver (PG-56). |
| `loop` | present | qtest `for:1`, `for:2`, `while:1`, `while:2`, `until:2` |
| `parameter-expansion` | present | qtest `p1:abc:bc:ab:3`, `p2:default`; host fixture `${x:=}`, `%`, `%%` |
| `pathname-expansion` | present | qtest `/tmp/p4g-a /tmp/p4g-b` |
| `pipeline` | present | qtest `rshpiped`, `rshstatus:0`, `rshstatus:1`: builtin stages via P2j pipes and P2n fd maps; each stage is a child `rsh -c`. |
| `quote-removal` | present | qtest `quote:a b`; host fixture quoting forms |
| `redirection` | present | qtest `rshredir`, `rshappended`, `rsh: cat: /nonexistent`, `rsh: cat: /missing`: `<`, `>`, `>>`, `2>`, `2>&1` through rsh's `file:` handles; brush's stdio defect is recorded below. |
| `regular-builtin` | present | qtest `read:`, `eval:ok`, `umask:0022`; host fixture `printf`/`test` |
| `set-options` | present | qtest `errexit:1`, `nounset:1`, `xtrace:0` |
| `shell-variable` | present | qtest `simple:1:2`, `ro:1`; host fixture env/export |
| `simple-command` | present | qtest `simple:1:2`; host fixture arguments/assignments |
| `special-builtin` | present | qtest `eval:ok`, `trap:exit`, `ro:1`, `umask:0022`, `getopts`; host `.`/`exec`/`readonly` |
| `trap` | waived | EXIT traps pass (`trap:exit`); signal traps need PG-7 signal delivery (USR1 currently exits the shell). |
| `umask` | present | qtest `umask:0022`; host symbolic modes |

## Findings and follow-ups

1. **Brush target stdio remains a platform defect.** `echo x > /tmp/f` in a fresh brush
   process returns `error: echo: i/o error: failed to write whole buffer`; reading a file created
   by rsh and then writing to stdout also fails once. The exact seam is the ravenlibc fd table /
   `std::fs::File` path in `libc/ravenlibc/crates/raven-libc-platform` (the first `open` can take
   a stdio descriptor, so the following `write` gets a short/zero completion). This is owner
   libc-lane follow-up work, not shell parsing; the rsh rows below use raw `file:` handles and
   are unaffected. The P4a brush evidence for the first 18 rows is still green.
2. **P4p implementation.** `crates/raven-shell-core` now parses pipelines, `2>`, `2>&1`,
   here-documents and lists; rsh runs each pipeline stage as a child `rsh -c` with P2j pipes and
   P2n fd maps, applies redirections with its `file:` handles, feeds here-doc bodies through a
   pipe, and parses the initfs lazily so builtin-only stages do not pay the 27 MiB CRC cost.
3. **Platform waivers.** PG-56 (job control, asynchronous `$!`/`wait pid`), PG-7 (signal traps)
   and P9c (interactive login) remain written waivers; this ticket does not claim them.
4. **rsh.** rsh is the release shell and carries the four new rows; brush remains the POSIX
   syntax reference for the 18 existing rows. No donor shell source was read or copied.

## UNVERIFIED

No Pi 4/Pi 5 board run is claimed; the target evidence is QEMU x86_64.
