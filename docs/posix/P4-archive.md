<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4j - POSIX archive tools

First-party Rust implementations of `ar`, `uuencode` and `uudecode`, plus host behaviour tests
and executable evidence rows.

## Method

* `ar` implements the portable `!<arch>\n` archive: 60-byte headers, short `name/` members,
  `#1/<len>` long names, `r` (replace/insert), `t` (list), `x` (extract) and `s` (rewrite the
  symbol table). `x` refuses path-like member names and restores the stored mode.
* The `/` symbol table is generated for ELF32/ELF64 little-endian object members from their
  `.symtab`/`.strtab`; non-ELF members are still valid and simply contribute no symbols.
* `uuencode` writes the POSIX `begin <mode> <name>` / body / backtick / `end` format with 45-byte
  lines. `uudecode` validates the header, length characters, groups and mode, and rejects unsafe
  encoded file names.
* The target binaries use `raven_rt::fs`; the host tests and the `test:` evidence exercise the
  same pure code. Standard input is not available in the Raven image, so the target wrappers
  require a file operand and print an explicit usage error otherwise.

## Tool matrix

| Tool | Format coverage | Tests |
|---|---|---:|
| `ar` | `r`/`t`/`x`/`s`; short and long names; modes; ELF symbol table; malformed headers | 6 lib + 1 host |
| `uuencode` | begin/end wrapper; 45-byte chunks; empty/short/partial input; mode/name | 6 lib + 2 host |
| `uudecode` | header/mode/name validation; safe names; malformed end/length | 3 lib + 1 host |

All three `util.*` rows are registered in `docs/posix/evidence/P4-archive.toml`.

## Known defects / limits

| Defect | Tool | Detail |
|---|---|---|
| `P4j-D1` | `ar` | `d`, `m`, `p`, `q` and `-u` are not implemented; unknown operations are a usage error, never silent wrong output |
| - | `ar` | the symbol table covers ELF32/ELF64 little-endian only; other object formats contribute no symbols |
| - | `uuencode`/`uudecode` | target standard input is unavailable in this image; file operands are required |
| - | `ar` | extraction is a simple current-directory write; no overwrite/owner policy beyond mode restoration |

## Acceptance

```
sh tests/posix/utility/run-archive.sh
cargo nextest run -p ar -p uuencode -p uudecode
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

The archive runner reports `3 pass, 0 fail`; the gate executes each `test:` script and registers
the three rows. No board claim.
