<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K485 line-discipline evidence

Tree: `a1a09cebeda9af63d53f73679fdb9a251be388a9` (branch `jorge/K485`), 2026-11-22.
The guest case `tty-discipline` drives every row through `tcsetattr`/`tcgetattr` and
`read`/`write` on a `posix_openpt` pair; no row asserts a stored bit without using it.

## Rows

| Row | Setting / input | Expected | Observed (green) |
|---|---|---|---|
| canonical-erase | canonical, `abc<VERASE>d\n` | `abd\n` | `abd\n` |
| canonical-kill | canonical, `ab<VKILL>cd\n` | `cd\n` | `cd\n` |
| canonical-werase | canonical+IEXTEN, `one two<VWERASE>X\n` | `one X\n` | `one X\n` |
| canonical-eof | `abc<VEOF>` then `VEOF` | `abc\n`, then read 0 | `abc\n`, then 0 |
| isig-intr | `ISIG`, `VINTR` then `abc\n` | `^C\r\n` echo, `abc\n` data, no `^C` data | as expected; `sigint=pgrp-unset` |
| isig-quit | `ISIG`, `VQUIT` then `abc\n` | `^\r\n` echo, `abc\n` data, no `^\\` data | as expected; `sigquit=pgrp-unset` |
| opost-onlcr | slave `A\n`, `OPOST\|ONLCR` | master `A\r\n` | `A\r\n` |
| opost-off | slave `A\n`, no `OPOST` | master `A\n` | `A\n` |
| icrnl | raw `ICRNL`, master `\r` | slave `\n` | `\n` |
| inlcr | raw `INLCR`, master `\n` | slave `\r` | `\r` |
| igncr | raw `ICRNL\|IGNCR`, master `\r`, `VMIN=0,VTIME=1` | no data | no data |
| istrip | raw `ISTRIP`, master `0xff` | slave `0x7f` | `0x7f` |
| vmin1 | `VMIN=1,VTIME=0`, one queued byte | 1 byte, no spin | `read=1 byte=88`, 7 ms |
| vtime | `VMIN=0,VTIME=1`, empty | 0 after ~100 ms | `count=0`, 110 ms |

The foreground-process-group delivery itself is not measured: libc has no `tcsetpgrp`
(PG-54) and the kernel job-control path is outside this ticket. The case prints
`pgrp-unset not-measured`; the ptyd mapping is unit-tested (`SIGQUIT = 3`).

## Red and green

`qtest-red.*` is the pre-fix guest run: `tty-discipline: FAIL rows=7`
(canonical-werase passed `^W` through; isig-quit-data `1c616263`; icrnl `0d`; inlcr `0a`;
igncr byte 13; istrip `ff`; vtime `count=-1`). `qtest-final.*` is the green acceptance run
(`result: pass`, 55.394 s, 18 `ok` rows). `tty-core-red.log` and `ptyd-red.log` are the
host-test red runs (4 tty-core failures, 1 ptyd SIGQUIT failure); `unit-green.log` is
79/79 after the fix.

## Mutation

* `mutation-vwerase.log`: removing the `VWERASE` branch makes
  `vwerase_removes_the_previous_word` fail (61 passed, 1 failed).
* `mutation-sigquit.log`: removing the ptyd `output.quit` mapping makes
  `ctrl_backslash_signals_the_foreground_group_and_echoes` fail (16 passed, 1 failed).

Both mutations were reverted; the committed tree is the green tree.

## Gates

| Command | Cap | Exit | Result |
|---|---:|---:|---|
| `cargo xtask qtest --case tty-discipline --arch x86_64` | 300 s | 0 | pass, 55.394 s |
| `cargo nextest run -p raven-tty-core` | 180 s | 0 | 62/62 |
| `make check-fast` | 900 s | 0 | `check-fast: PASSED` |
| `cargo xtask qtest --case pty-calls --arch x86_64` | 300 s | 0 | regression pass |
| `cargo xtask qtest --case tty-control --arch x86_64` | 300 s | 0 | regression pass |

See `SHA256SUMS` for every file in this directory.
