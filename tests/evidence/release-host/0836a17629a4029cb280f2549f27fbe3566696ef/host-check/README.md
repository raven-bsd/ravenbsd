<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 0.1.0 full host check on `0836a176` (K420, green)

- Base: `origin/master` `0836a17629a4029cb280f2549f27fbe3566696ef` (K421, K422, K423, K424 and
  K425 merged).
- Branch: `bob/K420` merged with `origin/master` at `db0c19a1`; the product tree is `0836a176`.
- Command: `timeout 250 make check` (one run; the K420 round-2 measured 176.87 s and K416 245 s,
  so the cap is 250 s).
- Result: **`check: PASSED`** — exit 0, 192.11 s.

## Steps

| Step | Exit | Seconds | Result |
|---|---:|---:|---|
| guard | 0 | 7.21 | ok |
| fmt | 0 | 9.71 | ok |
| clippy (host) | 0 | 0.61 | ok |
| tests (host, parallel) | 0 | 96.87 | ok: 13009 run, 13009 passed (1 slow), 4 skipped |
| tests (doc) | 0 | 6.13 | ok |
| profiles | 0 | 0.24 | ok |
| provenance | 0 | 68.94 | ok |
| clippy kernel x86_64 | 0 | 0.28 | ok |
| clippy kernel aarch64 | 0 | 0.21 | ok |
| clippy user programs x86_64 | 0 | 0.63 | ok (K421–K425 closed) |
| clippy loader x86_64 | 0 | 0.27 | ok |
| docs | 0 | 0.32 | ok |

```text
check summary:
  ...
  ok       clippy user programs x86_64 (exit 0, 0.63s)
  ...
check: PASSED
```

There are no failures; `failures.tsv` carries only its header. `check-fast` is not evidence and was
not used as a substitute; the full gate above is the proof.

## Artifacts

| File | Contents |
|---|---|
| `make-check.log` | complete `make check` output (24,038 bytes) |
| `summary.tsv` | per-step exit, seconds, result |
| `failures.tsv` | header only (no failures) |
| `SHA256SUMS` | hashes of the files above |

The parent `docs/releases/0.1.0-host-check.md` records the K416/K420 history.
