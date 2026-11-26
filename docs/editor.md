<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# The `ed` editor on the RavenBSD console (K186)

Status: OBSERVED; `/bin/ed` reads the login tty session owned by `consd` and processes commands
incrementally.

## Input ownership

`consd` owns the serial input ring and serves it as `tty:console`. The first `ed` wrapper read
`raven_rt::debug_read` directly, so it competed with `consd` for the same bytes: `consd` took
the typed script, `ed` timed out, and the shell executed the leftovers as commands.

`ed` now follows the `bc`/`m4` precedent and opens the same session:

```rust
let mut console = fs::open("tty:console", 0)?;
```

It keeps a partial-line buffer and executes each complete line as it arrives. An `a`, `i` or
`c` block stays open between reads until its terminating `.`, and `q` returns immediately
without waiting for an end-of-input marker. Output still goes through the console writer
(`debug_write`); the editor never reads the kernel debug ring.

The `-s` option and the POSIX command subset are unchanged.

## Staging

`ed` is built as a static `/bin` tool so the shell can spawn it through the no-interpreter path:

* `profiles/shell.d/60-rsh.toml` and `profiles/pi4-release.d/20-userspace.toml` stage
  `package = "ed"` at `/bin/ed`;
* `xtask/src/initfs.rs` lists `ed` in `STATIC_SHELL_PACKAGES`.

## Scripted proof

`tests/qtest/cases/ed-console.toml` logs into the release image and types two real sessions:

1. append `alpha one` / `beta two`, `.`, `w /tmp/ed-console.txt`, `q`;
2. re-open the file, `1s/alpha/ALPHA/`, append `gamma three`, `w`, `q`;
3. `cat` the file and assert `ALPHA one`, `beta two`, `gamma three`.

`ALPHA one` was never typed, so the assertion proves the file content rather than the console
echo. The prompt after `q` proves the editor exits without an EOF marker.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| Line editor input | terminal/STDIN semantics; CDDL behaviour only | NetBSD `bin/ed` reads stdin/stdout; BSD-3-Clause reference, no code copied | GNU ed reads stdin; outcome only | read the `tty:console` session, process lines incrementally, keep the existing Rust editor logic |
| Console ownership | console/STREAMS discipline; behaviour only | tty layer owns input; no code copied | tty/line discipline; outcome only | one input owner: `consd` serves `tty:console`; `ed` reads that handle, never the debug ring |
| Scripted proof | not applicable | ed command scripts are the editor's normal interface | same | qtest types the script at the console and reads the written file |
