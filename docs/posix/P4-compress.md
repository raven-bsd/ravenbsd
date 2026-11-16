<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4n - POSIX compress/uncompress/zcat

First-party Rust implementations of the historical `.Z` LZW format, with host tests and
executable evidence rows.

## Method

* `compress` writes the `1f 9d` magic and a block-mode flags byte (`maxbits | 0x80`), 9..16-bit
  LSB-first LZW codes, dictionary clear when the table fills, `-c` stdout and `-d` decompression.
* `uncompress` decodes `.Z` streams, accepts an optional `.Z` suffix, writes the stripped name and
  removes the compressed input unless `-c`.
* `zcat` decodes one or more `.Z` files to standard output.
* The target wrappers use `raven_rt::fs`; `-f`/file operands are required because standard input
  and output archive streaming is not available in this image.
* Host codec tests include a real `gzip -dc` interoperability check when `gzip` is available.

## Tool matrix

| Tool | Covered | Tests |
|---|---|---:|
| `compress` | header/flags; LZW 9..16-bit widths; empty/text/binary round-trip; clear/full table; corrupt input | 5 lib + 1 host |
| `uncompress` | `-c`, file operands, `.Z` suffix stripping | 2 lib |
| `zcat` | file operands and `.Z` source naming | 2 lib |

All three `util.*` rows are registered in `docs/posix/evidence/P4-compress.toml`.

## Unsupported / limits

| Item | Detail |
|---|---|
| target streams | target standard input/output archive streaming is not implemented; file operands are required |
| mode/time | file mode is not restored by the current wrappers; no utimes primitive exists |
| non-block mode | only block-mode `.Z` headers with clear codes are accepted |
| concatenated streams | one member per file; concatenated `.Z` members are not joined |

## Acceptance

```
sh tests/posix/utility/run-compress.sh
cargo nextest run -p compress -p uncompress -p zcat
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

The runner reports `3 pass, 0 fail`; the gate executes the registered `test:` cases. No board
claim.
