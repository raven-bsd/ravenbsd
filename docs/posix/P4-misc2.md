<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4z - echo, date and man evidence

Registration and behaviour evidence for the existing first-party `echo`, `date` and `man`
utilities. No utility implementation was changed.

## Method

* The target programs are `no_std` wrappers that call a pure library; the host `main` is a stub
  because the real entry point needs the Raven kernel. The P4z cases therefore exercise the same
  pure argument and formatting APIs that the wrappers call.
* `tests/posix/utility/misc2/evidence` is a small host-only workspace crate with one integration
  test file per tool. It depends on `raven-echo`, `raven-date` and `man`; it is not a product crate.
* `tests/posix/utility/run-misc2.sh` runs each `test.sh` in a fresh log. Every case uses
  `cargo nextest`, because nextest is mandatory for Rust tests.
* No vendored source or utility binary was edited.

```sh
sh tests/posix/utility/run-misc2.sh
sh tests/posix/utility/run-misc2.sh echo
sh tests/posix/utility/run-misc2.sh --list
```

## Tool matrix

| Tool | Cases | Row | Coverage |
|---|---:|---|---|
| `echo` | 4 | `util.echo` | `-n` only as the first operand; empty operands and `--`; literal backslashes; success/error status |
| `date` | 4 | `util.date` | fixed-epoch uptime values; `--selftest`; usage errors; `-u`/`+format` pinned as `P4z-D1` |
| `man` | 3 | `util.man` | fixture mdoc rendering; section/name validation; unknown-page lookup and clean no-candidate result |

All three `test:` scripts pass. The evidence rows are in `docs/posix/evidence/P4-misc2.toml`.

## Known defects / limits

| Defect | Tool | Detail |
|---|---|---|
| `P4z-D1` | `date` | Protocol version 1 has no wall clock. The utility prints `up <seconds>s` and accepts only `--selftest`; POSIX `-u` and `+format` are usage errors. The case pins the gap instead of claiming UTC support. A wall-clock source and the POSIX forms need a separate ticket. |
| - | `echo` | Backslash escapes are not processed. POSIX leaves `echo` escape handling implementation-defined; this implementation prints operands literally. `-n` is recognised only as the first operand. |
| - | all three | Host-level evidence only; the target wrappers, console and pager paths need the Raven kernel. No board claim. |

## Acceptance

```
sh tests/posix/utility/run-misc2.sh
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

The runner reports `3 pass, 0 fail`; the gate executes the three `test:` scripts and reports
`util.echo`, `util.date` and `util.man` as present. The `date` row carries `P4z-D1` in its note.
