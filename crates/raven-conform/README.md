<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-conform

Conformance result parsing and `KNOWN_FAILURES` enforcement for pjdfstest and LTP. The crate
builds the host tool `raven-conform` and exposes the parsers as a library.

## Usage

```text
raven-conform parse pjdfstest <file> [--target <name>] > results.json
raven-conform parse ltp <file> [--target <name>] > results.json
raven-conform compare --known <KNOWN_FAILURES> <results.json> [--allow-new]
raven-conform summary <results.json>... [--json]
```

`parse` writes the unified JSON schema (`tool`, `tool_version`, `target`, `unparsed`, `results`).
`results` is sorted by suite and name; duplicate names within a suite get `#2`, `#3`, ...
`unparsed` counts non-comment lines that did not match a known format.

## pjdfstest input

The parser recognizes the TAP-like output produced by `tests/misc.sh`:

* `ok N`, `ok N - description`, `not ok N - description`;
* `1..M` plan lines;
* `# SKIP` and `# TODO` directives (mapped to `skip`);
* `prove -v` file markers `tests/<area>/<n>.t ..`, and summaries `.. ok` or
  `.. Failed N/M subtests`, which become the suite name;
* comments, `Result:`, `Files=`, `Tests=` and `Bail out!` (the last becomes one `broken` result).

Without a `prove` file marker, results use the suite `pjdfstest`.

## LTP input

* `ltp-pan` lines:
  `tag=<name> stime=.. dur=.. exit=.. stat=.. core=.. cu=.. cs=..`.
  `exited/0` is `pass`, `exited/1` is `fail`, `exited/2` is `broken`, `exited/32` is `skip`;
  a `signaled` exit is `broken`. `dur` is converted from seconds to milliseconds.
* Plain output tags `TPASS:`, `TFAIL:`, `TCONF:`, `TBROK:`; the prefix before the tag is the
  test name. `TWARN:` and `TINFO:` are diagnostics and do not produce results.

## KNOWN_FAILURES

One entry per line:

```text
<suite>:<test name>  # reason
```

Blank lines and lines starting with `#` are ignored. A test name may contain `#2`; only a
`#` preceded by a space starts the reason comment.

`compare` results:

| Situation | Exit |
|---|---|
| no regression, no fixed entry | 0 |
| failing/broken test not listed | 1 |
| listed test now passes | 3 |
| `--allow-new` and a new failure | 4 (with a warning; never 0) |

`--allow-new` prints `WARNING: the lead refuses additions without justification` and labels new
failures `NEW-KNOWN`; it does not make them acceptable.

## Fixtures

All files under `tests/fixtures/` are **synthetic**: they were written by hand from the donor
output conventions and were not produced by running pjdfstest or LTP. No GPL code was copied.
The pjdfstest files cover direct TAP, a passing `prove` file and a failing `prove` file; the LTP
files cover `ltp-pan` and plain tag output.
