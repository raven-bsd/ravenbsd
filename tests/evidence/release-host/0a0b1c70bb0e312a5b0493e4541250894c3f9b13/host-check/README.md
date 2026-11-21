<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 0.1.0 full host check on `0a0b1c70` (K420)

- Base: `origin/master` `0a0b1c70bb0e312a5b0493e4541250894c3f9b13` (merge of K418, includes K419
  `c3ffda69`).
- Command: `timeout 300 make check` (one run; the K416 measured runtime was 245.16 s, so the cap
  was 300 s).
- Result: **`check: FAILED`** — exit 2, 173.72 s. The host test suite is green; the only red step
  is a new pre-existing clippy lint in `ed` (not K418/K419).
- Fast gate: `timeout 240 make check-fast` — exit 0, 175.55 s, `check-fast: PASSED`. The fast gate
  skips the four target-clippy steps, so it is **not evidence** for this ticket.

## Steps

| Step | Exit | Seconds | Result |
|---|---:|---:|---|
| guard | 0 | 16.32 | ok |
| fmt | 0 | 13.49 | ok |
| clippy (host) | 0 | 4.27 | ok |
| tests (host, parallel) | 0 | 77.73 | ok: 13009 run, 13009 passed, 4 skipped |
| tests (doc) | 0 | 5.19 | ok |
| profiles | 0 | 0.12 | ok |
| provenance | 0 | 45.18 | ok |
| clippy kernel x86_64 | 0 | 2.65 | ok (K418 closed) |
| clippy kernel aarch64 | 0 | 2.56 | ok (K418 closed) |
| clippy user programs x86_64 | **101** | 3.60 | **FAILED: `ed` `while_let_on_iterator`** |
| clippy loader x86_64 | 0 | 0.73 | ok |
| docs | 0 | 0.29 | ok |

## New failure

```text
error: this loop could be written as a `for` loop
   --> userland/bin/ed/src/lib.rs:220:9
    |
220 |         while let Some(line) = lines.next() {
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: try: `for line in lines`
    = help: #while_let_on_iterator
    = note: `-D clippy::while-let-on-iterator` implied by `-D warnings`
```

The K418 kernel-exec and K419 devd lints are closed. This `ed` lint is a new pre-existing
full-check blocker and needs its own ticket; K420 makes no source change.

Owner proposal: Phil (full-check gate) or Kevin (`userland/bin/ed`), ticket `K421
ed-clippy-while-let`.

## Artifacts

| File | Contents |
|---|---|
| `make-check.log` | complete `make check` output (45,851 bytes) |
| `check-fast.log` | complete `make check-fast` output (not evidence; fast gate skips target clippy) |
| `summary.tsv` | per-step exit, seconds, result |
| `failures.tsv` | crate, step, lint, file:line, owner proposal |
| `SHA256SUMS` | hashes of the files above |

The parent `docs/releases/0.1.0-host-check.md` records the K416/K420 history.
