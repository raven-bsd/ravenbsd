<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4t - POSIX ed

First-party Rust `ed` line editor with a host-testable command model.

## Command matrix

| Area | Covered |
|---|---|
| addresses | line numbers, `.`, `$`, `'x` marks, `/re/`, `?re?`, comma ranges, `%`, `+/-` offsets |
| edit | `a`, `i`, `c`, `d`, `p`, `n`, `l` |
| substitute | `s` with basic regex, `g`, numeric occurrence and `p` flags |
| global | `g/re/command`, `v/re/command`; `p`/`n`/`d`/`s` command bodies |
| move/copy/join | `m`, `t`, `j` |
| marks | `k` with a one-character name |
| files | `r`, `w`, `e`, `f` through an `Io` trait |
| state | `u` one-level undo, `q`, `=` |
| diagnostics | invalid address/command/regex, missing input terminator, I/O errors |
| unsupported | shell escape `!` returns `Unsupported` |

## Method

* The pure `Editor` owns the buffer, current line, marks, one-level undo and output.
* `execute(script, io)` runs a newline-separated command script; `a`/`i`/`c` consume lines
  until a lone `.`. Host tests use an in-memory `Io` and a real-filesystem `Io`.
* The target wrapper reads `-s [file]`, opens the file through `raven_rt::fs`, reads command
  bytes from the serial console, and writes command output back to the console.
* The target image has no standard-input stream, so the wrapper uses `debug_read`; the host tests
  exercise the same command model without the console.

## Unsupported / limitations

| Item | Detail |
|---|---|
| shell escape | `!` is an explicit `Unsupported` error |
| standard input | no generic stdin handle in this image; target reads the serial console |
| multi-level undo | one snapshot (`u`) only |
| regex | POSIX basic regular expressions via `raven-libc-regex`; GNU extensions are not claimed |

## Acceptance

```
sh tests/posix/utility/run-ed.sh
cargo nextest run -p ed
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

`util.ed` is registered only after the host cases pass.
