<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P45 - shell-builtin POSIX utility rows

Registration and qtest evidence for the POSIX shell builtins. The existing `posix-shell` qtest was
extended with one builtin group; no shell source was changed.

## Method

* `tests/qtest/cases/posix-shell.toml` now runs an additional `/bin/brush --posix -c` group and
  one `exit 7` external-brush check, with concrete expectations for every claimed builtin.
* `tests/fixtures/posix-shell/posix-shell.sh` carries the same host-reference commands. The host
  brush output was checked separately; the target evidence is the qtest.
* Present rows use `qtest:posix-shell`. Waived rows carry the written waiver in
  `docs/posix/evidence/P4-shellbuiltins.toml`.
* No `userland/bin/rsh/**` or `userland/brush/**` source was edited.

## Builtin matrix

| `util.*` | State | qtest markers |
|---|---|---|
| `util.alias` | present | `alias:0` and `unalias:0`; alias define/list and remove |
| `util.bg` | waived | PG-56 job model |
| `util.break` | present | `break-loop:1`, `break:done` |
| `util.cd` | present | `cd:0` |
| `util.command` | present | `cmd:ok`, `command:0` |
| `util.continue` | present | `continue:1`, `continue:3` |
| `util.dot` | present | `dot:ok` after `. /tmp/p4b-dot` |
| `util.eval` | present | `eval:ok` |
| `util.exec` | present | `exec-read` from `exec 3<`; `exec 3<&-` closes the descriptor |
| `util.exit` | present | `exit:7`, final `init exited (0)` |
| `util.export` | present | `export:yes` |
| `util.fc` | waived | interactive history editing; non-interactive qtest |
| `util.fg` | waived | PG-56 job model |
| `util.getopts` | present | `getopt:a:`, `getopt:b:val`, `optind:4` |
| `util.hash` | present | `hash:0` for the no-argument reset/list path |
| `util.jobs` | waived | PG-56 job model |
| `util.read` | present | `read:...`, `exec-read:...` |
| `util.readonly` | present | `ro:1` |
| `util.return` | present | `func-status:3`, `return:7` |
| `util.set` | present | `errexit:1`, `nounset:1`, `xtrace:0`, `set --` |
| `util.shift` | present | `shift:1:b` |
| `util.test` | present | `test:0`, `bracket:0` |
| `util.times` | unclaimed | target page fault in the `times` builtin; see `P45-D1` |
| `util.trap` | present | `trap:exit` |
| `util.type` | present | `type:0` |
| `util.ulimit` | unclaimed | `ulimit -n` reports unsupported resource; see `P45-D2` |
| `util.umask` | present | `umask:0022` |
| `util.unalias` | present | `unalias:0` |
| `util.unset` | present | `unset u` / `unset IFS` feed `p2:default` and `split` |
| `util.wait` | waived | PG-56 job model |

Totals: **23 present, 5 waived, 2 unclaimed**.

## Defects / limits

| Defect | Builtin | Detail |
|---|---|---|
| `P45-D1` | `times` | `brush --posix -c 'times; echo times:$?'` kills the target child with a page fault at `0x0`; the host brush prints the times table. The qtest does not claim the row. |
| `P45-D2` | `ulimit` | `ulimit -n` returns `ulimit: i/o error: unsupported resource` and status 1 on the target; the row is not claimed. |
| - | `alias` | Definition, listing and removal are tested; alias substitution in a single non-interactive `-c` parse unit is not claimed (the host bash behaves the same for one `-c` string). |
| - | `hash` | Only the no-argument reset/list path is tested; resolving/hashing a command name is not claimed. |
| - | `bg`/`fg`/`jobs`/`wait` | PG-56 asynchronous lists, `$!` and the job table are not implemented. |
| - | `fc` | Interactive history editing is outside the non-interactive qtest and the P9c login surface. |

## Acceptance

```
cargo xtask qtest --case posix-shell --arch x86_64
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

The qtest passes after the builtin group; the gate executes `qtest:posix-shell` for every present
row. No board claim.
