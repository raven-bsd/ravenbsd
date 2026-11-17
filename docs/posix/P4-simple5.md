<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4j - POSIX lp client

First-party Rust implementation of the POSIX.1-2024 `lp` client, with host tests, a runner, an
executable evidence row and a man page. RavenBSD has no print daemon and no network print
service; the client reports the documented diagnostics instead of pretending that a page was
printed.

## Destination model

* `-d dest` takes precedence over `LPDEST`, which takes precedence over `PRINTER`. An empty
  environment value is treated as unset.
* A destination that is an absolute path or an explicit scheme path (`/dev/lp0`,
  `tty:console`, `file:/tmp/out`) is opened for writing with `O_WRONLY|O_TRUNC` and the request
  is written synchronously. The destination must already exist; `lp` never creates it.
* Every other destination is a logical printer name. The image has no print service behind such
  a name, so `lp` exits 1 with the unavailable diagnostic.
* With no destination at all, `lp` exits 1 with the no-destination diagnostic before reading any
  input, so a configured-but-unavailable or unset destination cannot hang on standard input.

## Option matrix

| Option | Behaviour | Status |
|---|---|---|
| `-c` | Input is copied before writing; the client always reads each document fully into memory | supported |
| `-d dest` | Destination selection with the POSIX precedence | supported |
| `-n copies` | Positive decimal copy count; every complete document sequence is repeated | supported |
| `-m` | Accepted; no mail is sent because the image has no mail transport | explicit defect `P4j-D2` |
| `-o option` | Accepted and kept in request order; a local file or device has no device-dependent options | explicit defect `P4j-D3` |
| `-s` | Suppresses the `-w` completion message; diagnostics still go to standard error | supported |
| `-t title` | Accepted; no banner page is generated for a synchronous local write | explicit defect `P4j-D4` |
| `-w` | After a successful write, prints `lp: request printed on '<dest>'` to standard error unless `-s` | supported |

## Input and output

* File operands are read in order; no operands, or a lone `-`, selects standard input (the
  console in the target image).
* A read error prints `lp: <path>: <errno text>` or `lp: standard input: <errno text>` and exits
  1.
* A write error prints `lp: <dest>: <errno text>` and exits 1.
* No request ID is written to standard output: the image has no spooler and the local write is
  synchronous. This is explicit defect `P4j-D1` against the POSIX STDOUT requirement.

## Exact diagnostics

| Condition | Standard error | Exit |
|---|---|---:|
| No destination from `-d`, `LPDEST` or `PRINTER` | `lp: no print destination configured` | 1 |
| Logical printer name with no service | `lp: destination '<dest>' is not available` | 1 |
| Local destination cannot be opened | `lp: destination '<dest>' is not available: <errno text>` | 1 |
| Usage error | message from the parser plus `usage: lp ...` | 2 |

## Defects

| Defect | Detail |
|---|---|
| `P4j-D1` | no request ID on standard output; no spooler, `cancel` or `lpstat` exists in the image |
| `P4j-D2` | `-m` sends no mail; there is no mail transport |
| `P4j-D3` | `-o` is carried but a local file or device has no device-dependent options |
| `P4j-D4` | `-t` is carried but no banner page is generated |
| - | input is held in memory; very large files are bounded by available memory |
| - | logical printer names always fail; only absolute or scheme paths can accept a request |

## Tests

`tests/posix/utility/simple5/lp/test.sh` runs the `lp` package with nextest. The host tests cover
the default request, every option in separate, attached and grouped spellings, `--` and `-`
handling, usage errors, copy counts, destination precedence, destination classification, copy
ordering and the exact diagnostic strings.

## Acceptance

```
sh tests/posix/utility/run-simple5.sh
cargo nextest run -p lp
cargo xtask posix-check --profile posix --arch x86_64 --host-only
make check-fast
```

The runner reports `1 pass, 0 fail`; `util.lp` is registered in
`docs/posix/evidence/P4-simple5.toml`. No board claim.
