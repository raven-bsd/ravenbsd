<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4a POSIX shell semantics (P4-shell)

Status: P4a implementation. The host `brush --posix` conformance transcript matches GNU
bash 5.1.16 `--posix` for every marker in `tests/fixtures/posix-shell/posix-shell.sh` except
`$$`. The target `posix-shell` qtest passes the supported grammar, expansion, redirection,
function, loop, set-option, trap-EXIT, builtin and `getopts` groups. The P0b overlay is merged;
`docs/posix/evidence/P4-shell.toml` registers the 18 passing rows against
`qtest:posix-shell` and the 7 genuinely missing behaviours as written waivers. The gate executes
the qtest for each present row.

## Evidence

```sh
# Host: the full fixture, one marker per row family.
userland/brush/target/debug/brush --posix tests/fixtures/posix-shell/posix-shell.sh

# Target: the four supported groups on the opt-in rsh+brush image.
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
| `here-document` | waived | Target script/source stdout returns `failed to write whole buffer` (P4a finding); the host fixture proves `<<` and `<<-`. Follow-up: target script-mode stdio. |
| `interactive-shell` | waived | P9c; the interactive login is the `modern-shell` profile, not this host gate. |
| `job-control` | waived | PG-56 (kernel process groups/sessions/tty control). |
| `list` | waived | Sequence lists are present and tested; asynchronous `$!`/`wait pid` need the job model (PG-56). |
| `loop` | present | qtest `for:1`, `for:2`, `while:1`, `while:2`, `until:2` |
| `parameter-expansion` | present | qtest `p1:abc:bc:ab:3`, `p2:default`; host fixture `${x:=}`, `%`, `%%` |
| `pathname-expansion` | present | qtest `/tmp/p4g-a /tmp/p4g-b` |
| `pipeline` | waived | PG-35 `pipe2` is ENOSYS. |
| `quote-removal` | present | qtest `quote:a b`; host fixture quoting forms |
| `redirection` | waived | Input `<`, `exec 3<`, `exec 3<&-` and `>&-` pass; target `>`/`>>` to a file returns `failed to write whole buffer` (P4a finding, follow-up). |
| `regular-builtin` | present | qtest `read:`, `eval:ok`, `umask:0022`; host fixture `printf`/`test` |
| `set-options` | present | qtest `errexit:1`, `nounset:1`, `xtrace:0` |
| `shell-variable` | present | qtest `simple:1:2`, `ro:1`; host fixture env/export |
| `simple-command` | present | qtest `simple:1:2`; host fixture arguments/assignments |
| `special-builtin` | present | qtest `eval:ok`, `trap:exit`, `ro:1`, `umask:0022`, `getopts`; host `.`/`exec`/`readonly` |
| `trap` | waived | EXIT traps pass (`trap:exit`); signal traps need PG-7 signal delivery (USR1 currently exits the shell). |
| `umask` | present | qtest `umask:0022`; host symbolic modes |

## Findings and follow-ups

1. **Target output redirection is broken.** `echo x > /tmp/f` in brush `-c` mode returns
   `error: echo: i/o error: failed to write whole buffer`; `exec 3<>` and `>&3` work. The same
   class affects script/source mode stdout. This is outside the test-only part of P4a and needs
   a target file-open/stdio follow-up before `redirection`/`here-document` can be present.
2. **P0b overlay.** The gate reads `docs/posix/profile.toml` + `docs/posix/traceability.toml`;
   P0b's owner overlay loader accepts `docs/posix/evidence/P4-shell.toml`, so the gate executes
   `qtest:posix-shell` for the 18 present rows and keeps the 7 waivers visible in the report.
3. **Platform waivers.** PG-35 (pipeline), PG-56 (job control/async lists), PG-7 (signal traps)
   and P9c (interactive login) are the explicit reasons. They stay in `GAPS.md`; this ticket
   does not claim them.
4. **rsh.** The rescue rsh remains the 0.1.0 login shell; this ticket targets brush `--posix`
   and does not retire rsh. The release path is unchanged.

## UNVERIFIED

No Pi 4/Pi 5 board run is claimed; the target evidence is QEMU x86_64.
