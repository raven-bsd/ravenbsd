<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# qtest case files

`raven-qtest` reads one TOML file per test under `tests/qtest/cases/`. This is the contract C6
format; `raven-qtest list` prints every case it can parse.

## Required fields

| Key | Type | Meaning |
|---|---|---|
| `name` | string | Case name; also the image/log/JSON basename. No path separators. |
| `arch` | string | `x86_64`, `aarch64` or `riscv64`. |
| `profile` | string | Build profile passed to `cargo xtask image --profile`. |
| `timeout_secs` | integer | Overall wall-clock limit; must be greater than zero. |
| `expect_exit` | string | `success`, `failure`, `any` or `timeout`. |

## Optional fields

| Key | Type | Meaning |
|---|---|---|
| `boot_cmdline` | string | Passed to `cargo xtask image --cmdline`. |
| `qemu_extra` | array of strings | Extra QEMU arguments appended after the fixed ones. |
| `[[expect]]` | table | `regex` plus optional `unordered` (bool, default `false`). |
| `[[forbid]]` | table | `regex`; any matching serial line fails the case. |

Unknown keys and invalid regexes are errors.

## Example

```toml
name = "boot-uname"
arch = "x86_64"
profile = "base"
timeout_secs = 20
boot_cmdline = "console=ttyS0 test-exit=1"
qemu_extra = []
expect_exit = "success"

[[expect]]
regex = '^RavenBSD kernel 0\.1 \(x86_64\)'

[[expect]]
regex = '^RavenBSD \S+ 0\.1 RavenBSD 0\.1 \(\S+\) #\d+: .+ x86_64$'

[[forbid]]
regex = '(?i)panic'
```

## Matching

* Regexes are searched in each line; they are not implicitly anchored. Add `^`/`$` when the whole
  line matters.
* `[[expect]]` entries are matched in file order. An ordered entry searches from the line after the
  previous ordered match.
* `unordered = true` searches all lines not already used by an earlier expectation.
* One serial line can satisfy at most one expectation.
* `[[forbid]]` is checked against every line, independent of expectations.

## Results and exit codes

`build/qtest/<name>.log` holds the full serial output and `<name>.json` holds:

```json
{
  "case": "boot-uname",
  "arch": "x86_64",
  "result": "pass",
  "duration_ms": 1234,
  "exit_status": 33,
  "matched": ["^RavenBSD kernel 0\\.1 \\(x86_64\\)"],
  "unmatched": [],
  "forbidden_hits": []
}
```

`result` is one of `pass`, `fail`, `timeout`, `crash`, `error`, `skipped`. QEMU status 33 is guest
success, 35 is guest failure, anything else is a crash. Missing QEMU or firmware produces `skipped`;
a failed image build or spawn is `error`. `raven-qtest run` exits 0 on pass, 77 on skipped and 1 on
any other result; `raven-qtest all` treats skipped as non-failure.

## Running without QEMU

`--runner <command...>` replaces the QEMU command with an arbitrary program (the crate's own tests
use `sh -c` scripts). Example:

```
cargo run -q -p raven-qtest -- run --case tests/qtest/cases/smoke.toml \
  --runner sh -c "printf 'hello\n'; exit 33"
```
