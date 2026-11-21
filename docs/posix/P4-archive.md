<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4j/P4l - POSIX archive tools

First-party Rust implementations of `ar`, `uuencode`, `uudecode`, ustar `tar` and pax, plus host
behaviour tests and executable evidence rows.

## Method

* `ar` implements the portable `!<arch>\n` archive: 60-byte headers, short `name/` members,
  `#1/<len>` long names, `r` (replace/insert), `t` (list), `x` (extract) and `s` (rewrite the
  symbol table). `x` refuses path-like member names and restores the stored mode.
* The `/` symbol table is generated for ELF32/ELF64 little-endian object members from their
  `.symtab`/`.strtab`; non-ELF members are still valid and simply contribute no symbols.
* `uuencode` writes the POSIX `begin <mode> <name>` / body / backtick / `end` format with 45-byte
  lines. `uudecode` validates the header, length characters, groups and mode, and rejects unsafe
  encoded file names.
* `tar` implements ustar create/list/extract for regular files, directories and symbolic links,
  with modes, zero uid/gid for a non-root process, mtime, nested paths and the 100-byte name plus
  155-byte prefix fields. Paths that do not fit are rejected, never truncated; PAX/long-name and
  compression are explicit errors.
* `pax` extends the same base with `x` extended headers: `<length> keyword=value` records carry
  long paths, long link targets, large sizes and negative mtimes. It implements `-w`, `-a`, `-r`,
  `-x`, `-t`, `-f`, `-v` and `-p`; compression is an explicit error.
* The target binaries use `raven_rt::fs`; the host tests and the `test:` evidence exercise the
  same pure code. Standard input is not available in the Raven image, so the target wrappers
  require a file operand and print an explicit usage error otherwise.

## Tool matrix

| Tool | Format coverage | Tests |
|---|---|---:|
| `ar` | `r`/`t`/`x`/`s`; short and long names; modes; ELF symbol table; malformed headers | 6 lib + 1 host |
| `uuencode` | begin/end wrapper; 45-byte chunks; empty/short/partial input; mode/name | 6 lib + 2 host |
| `uudecode` | header/mode/name validation; safe names; malformed end/length | 3 lib + 1 host |
| `tar` | ustar `-c`/`-t`/`-x`; regular/dir/symlink; modes/mtime/nested names; checksum and safe extraction | 5 lib + 1 host |
| `pax` | `-w`/`-a`/`-r`/`-x`/`-t`; long paths/link targets; extended size/mtime records; safe extraction | 5 lib + 1 host |

The three P4j `util.*` rows, the P4l `util.tar` row and the P4r `util.pax` row are registered in
`docs/posix/evidence/P4-archive.toml`. `tar` was added to the seed `UTILITIES` list and
`profile.toml`/`traceability.toml` were regenerated; the gate executes the tar case and reports
the row as present. `util.pax` already had a profile row and is now backed by its passing case.

## Known defects / limits

| Defect | Tool | Detail |
|---|---|---|
| `P4j-D1` | `ar` | `d`, `m`, `p`, `q` and `-u` are not implemented; unknown operations are a usage error, never silent wrong output |
| - | `tar` | long-name/PAX, compression (`-z`/`-j`/`-J`) and non-regular/directory/symlink types are explicit errors |
| - | `pax` | compression (`-z`/`-j`/`-J`) is an explicit usage error; large sizes are represented by pax records but the in-memory writer still needs the data in memory |
| - | `ar` | the symbol table covers ELF32/ELF64 little-endian only; other object formats contribute no symbols |
| - | `uuencode`/`uudecode` | target standard input is unavailable in this image; file operands are required |
| - | `ar` | extraction is a simple current-directory write; no overwrite/owner policy beyond mode restoration |

## Acceptance

```
sh tests/posix/utility/run-archive.sh
cargo nextest run -p ar -p pax -p raven-tar -p uuencode -p uudecode
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

The archive runner reports `5 pass, 0 fail`; the gate executes each registered `test:` script and
reports `util.tar` and `util.pax` as present. No board claim.
