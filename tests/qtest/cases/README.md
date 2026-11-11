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
| `[[oracle]]` | table | Host-side artifact check; see below. |

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

## Audio oracle

A case can ask the harness to judge a file QEMU wrote. The case passes the `-audiodev` options
itself; after QEMU exits the harness parses the WAV and runs every requested check:

```toml
qemu_extra = ["-audiodev", "wav,id=a0,path=build/qtest/x.wav", "-device", "intel-hda", "-device", "hda-output,audiodev=a0"]

[[oracle]]
kind = "wav"
path = "build/qtest/x.wav"
min_duration_ms = 500
min_rms = 0.01
tone_hz = 440.0
tolerance_percent = 2.0
max_gap_ms = 50
```

| Key | Type | Meaning |
|---|---|---|
| `kind` | string | Only `wav` is implemented. |
| `path` | string | Artifact path relative to the harness working directory, or absolute. |
| `min_duration_ms` | integer | File duration in milliseconds must be at least this. |
| `min_rms` | float | RMS over all samples, normalised to `0.0..=1.0`, must be at least this. |
| `tone_hz` | float | Expected dominant frequency; requires `tolerance_percent`. |
| `tolerance_percent` | float | Allowed frequency error as a percentage of `tone_hz`. |
| `max_gap_ms` | integer | Longest run of exactly-zero frames after the first non-zero frame. |

Only uncompressed PCM16 mono/stereo WAV is accepted. `data` size 0 or `0xFFFFFFFF` is treated as
"use the rest of the file", as QEMU writes it while the file is open. The dominant frequency is
estimated with a Hann-windowed FFT and parabolic interpolation. A missing or malformed file fails
the case with the reason.

Each check is printed after the case line and written to `build/qtest/<name>.oracle.json`:

```json
{
  "case": "audio",
  "oracles": [
    {
      "path": "build/qtest/audio.wav",
      "checks": [
        {"check": "min_duration_ms", "passed": true, "detail": "520.0 ms >= 500 ms"},
        {"check": "tone_hz", "passed": true, "detail": "dominant 440.02 Hz ..."}
      ],
      "error": null
    }
  ]
}
```

Any failed check turns a passing case into `fail`; the exit status is then 1 and the reason is in
the result detail.

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
