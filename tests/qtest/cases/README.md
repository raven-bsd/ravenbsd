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
| `skip` | string | Do not run the case; report `skipped` with this reason. |
| `pre` | array of strings | Shell commands run in order from the repository root before the image build; a non-zero exit is an `error`. |
| `initfs` | string | Prebuilt initfs installed instead of the profile's. |
| `loader_kernel` | string | Kernel ELF installed instead of the built kernel (loader test kernels). |
| `[[expect]]` | table | `regex` plus optional `unordered` (bool, default `false`). |
| `[[forbid]]` | table | `regex`; any matching serial line fails the case. |
| `[[oracle]]` | table | Host-side artifact check; see below. |
| `[[step]]` | table | Scripted serial dialogue step; see below. |

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

## Fixture images

Some cases need a guest other than the profile's `/init`. `pre` builds the fixture and `initfs`
(or `loader_kernel`) installs it into the per-case image before QEMU starts. The S24 spawn case:

```toml
name = "kernel-spawn"
arch = "x86_64"
profile = "base"
timeout_secs = 120
pre = ["sh tests/fixtures/s24-spawn/build.sh"]
initfs = "build/fixtures/s24-spawn/initfs.img"
boot_cmdline = "console=ttyS0 test-exit=1 s24-framecheck=1"
expect_exit = "success"
```

`pre` entries are shell commands (`sh -c`) run in order from the harness working directory (the
repository root for `cargo xtask qtest`) before the image is built; a non-zero exit makes the case
`error` and QEMU never starts. `initfs` and `loader_kernel` are paths relative to the same
directory (absolute paths are accepted). The harness passes them to the image builder:

```
cargo xtask image --arch <a> --profile <p> [--cmdline <s>] [--initfs <p>] [--kernel <p>] --output <img>
```

`--initfs` replaces `raven/initfs.img` and `--kernel` replaces `raven/kernel.elf` in the ESP; the
loader is always built. A supplied image (`raven-qtest run --image ...`) or `--no-build` wins: the
overrides are only applied when the harness builds the image itself.

Each fixture directory carries one `build.sh` that reproduces the recipe from its ticket report:
the fixture packages are built for `x86_64-unknown-none` in release mode with the recorded
`RUSTFLAGS` (custom linker script, `-C lto=no`), then `raven-initfs`'s `mkinitfs` packs
`build/fixtures/<name>/initfs.img`. The scripts are deterministic: same source and toolchain
produce the same bytes, and the initfs format carries no timestamps. The loader test kernel is the
exception: it is a kernel, not an initfs, so its script writes
`build/fixtures/testkernel/kernel.elf` and the case uses `loader_kernel`:

```toml
pre = ["sh tests/fixtures/testkernel/build.sh"]
loader_kernel = "build/fixtures/testkernel/kernel.elf"
```

## Serial dialogue

A case can type into the guest while it runs. `[[step]]` tables are executed in file order: a
`wait` regex matches a serial line (or the current partial line, so an unterminated prompt such as
`raven# ` works), and `send` writes the text to the guest's serial input. The harness starts the
process with a stdin pipe; under QEMU that is the same byte stream as `-serial stdio` (x86-64) and
the `-nographic` serial (aarch64), so the typed text reaches the guest console.

```toml
name = "shell-uname"
arch = "x86_64"
profile = "base"
timeout_secs = 60
boot_cmdline = "console=ttyS0 test-exit=1"
expect_exit = "success"

[[step]]
wait = '^RavenBSD login: $'
timeout_secs = 30

[[step]]
send = "root\n"
delay_ms = 100

[[step]]
wait = '^raven# $'
send = "uname\n"
delay_ms = 0

[[step]]
wait = '^RavenBSD \S+ 0\.1'
send = "exit\n"

[[expect]]
regex = '^RavenBSD \S+ 0\.1'

[[forbid]]
regex = '(?i)panic'
```

| Key | Type | Meaning |
|---|---|---|
| `wait` | string | Regex that a serial line or the current partial line must match. A matched line (or matched partial prefix and the rest of that line) is consumed and is not offered to a later `wait`. |
| `send` | string | Text written verbatim to the guest serial input; `\n` in TOML is a newline. |
| `delay_ms` | integer | Pause before `send`, in milliseconds. Default 50; must be shorter than the case timeout. |
| `timeout_secs` | integer | How long `wait` may take. Default the case timeout; must not exceed it. |

A step needs `wait`, `send` or both. Steps are matched against the live stream in order; a line
before a match is skipped, just like the ordered `[[expect]]` matching. A step that cannot finish
(either timeout) kills the process and fails the case with the step number, its regex and the last
20 serial lines. If the process exits first, the pending step is reported the same way.

Steps and `[[expect]]` coexist: `[[expect]]` still matches over every captured line after the run,
including lines a step consumed, so the two features do not interfere.

When the guest cannot be typed into yet (S27 `DEBUG_READ` not merged), use `skip`:

```toml
skip = "needs S27 (DEBUG_READ) and the echo-dialogue fixture image"
```

The case then reports `skipped` (exit status 77) with that note and never starts QEMU.

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
