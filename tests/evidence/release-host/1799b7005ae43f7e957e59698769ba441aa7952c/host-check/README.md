<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 0.1.0 full host check on `1799b700` (K420 re-run)

- Base: `origin/master` `1799b7005ae43f7e957e59698769ba441aa7952c` (K421/K422/K423 merged).
- Branch: `bob/K420` merged with `origin/master` at `cb5dede8`; the product tree is `1799b700`.
- Command: `timeout 250 make check` (one run; the K416 measured 245 s and the K420 first round
  173.72 s, so the cap is 250 s).
- Result: **`check: FAILED`** — exit 2, 176.87 s. The host suite and the previously reported lints
  are green; four further unowned clippy lints now surface in `virtio-net` and `vfsd`.

## Steps

| Step | Exit | Seconds | Result |
|---|---:|---:|---|
| guard | 0 | 8.77 | ok |
| fmt | 0 | 15.21 | ok |
| clippy (host) | 0 | 1.48 | ok |
| tests (host, parallel) | 0 | 86.82 | ok: 13009 run, 13009 passed, 4 skipped |
| tests (doc) | 0 | 6.41 | ok |
| profiles | 0 | 0.12 | ok |
| provenance | 0 | 55.02 | ok |
| clippy kernel x86_64 | 0 | 0.23 | ok |
| clippy kernel aarch64 | 0 | 0.26 | ok |
| clippy user programs x86_64 | **101** | 0.98 | **FAILED: virtio-net and vfsd lints** |
| clippy loader x86_64 | 0 | 0.24 | ok |
| docs | 0 | 0.25 | ok |

## New failures

```text
error: manual implementation of `.is_multiple_of()`
  --> userland/drivers/virtio-net/src/main.rs:772:24
error: manual implementation of `.is_multiple_of()`
  --> userland/drivers/virtio-net/src/main.rs:810:24
error: this expression creates a reference which is immediately dereferenced by the compiler
  --> userland/servers/vfsd/src/main.rs:286:50
error: this expression creates a reference which is immediately dereferenced by the compiler
  --> userland/servers/vfsd/src/main.rs:295:68
```

The two `virtio-net` hits are the K396 TX stall trace's `tx_waits % 100 == 0`. The two `vfsd`
hits are redundant `&suffix` borrows in the rooted-symlink path. Both are outside K420's
Owns; they need their own tickets (proposed **K424** virtio-net, **K425** vfsd) and are not edited
here.

## Artifacts

| File | Contents |
|---|---|
| `make-check.log` | complete `make check` output (32,806 bytes) |
| `summary.tsv` | per-step exit, seconds, result |
| `failures.tsv` | crate, lint, file:line, owner proposal |
| `SHA256SUMS` | hashes of the files above |

`check-fast` was not rerun this round: it skips the target-clippy steps and is not evidence; the
full gate's red is the target clippy. The parent `docs/releases/0.1.0-host-check.md` records the
K416/K420 history.
