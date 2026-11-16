<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# POSIX.1-2024 profile and gap matrix

`profile.toml` is the machine-readable requirement inventory. `traceability.toml` maps every
requirement to its P1-P5 owner, test and evidence file. `GAPS.md` is generated from both and from
the current `Symbols.list`, headers, `userland/bin` and profile data.

The inventory records identifiers and requirements only; no POSIX standard prose is copied. The
interface/header seed is derived from the committed ravenlibc census ring-0 POSIX/C/XSI names and
the one-line prototypes in the POSIX headers; utilities, shell features, filesystem, process and
tty behaviour rows are first-party requirement statements.

```sh
cargo run -q -p posix-conform -- seed   # regenerate profile.toml and traceability.toml
cargo xtask posix-gaps                  # validate and regenerate GAPS.md
cargo xtask posix-gaps --check          # validate ids/owners/evidence/classification and freshness
cargo nextest run -p posix-conform      # host tests for the model, gate and overlay
```

The profile assumes the full Shell and Utilities volume. No requirement is silently omitted:
owner waivers, when decided, are explicit `status = "waived"` rows with a written `waiver`.
x86-64 is the first gate platform; aarch64 and riscv64 are `pending` until their qtest gates
exist.

## Owner evidence overlays

Owners close rows independently in `docs/posix/evidence/<owner>.toml` (`P1.toml` through
`P5.toml`). `docs/posix/evidence.toml` is an optional aggregate file; per-owner files are
preferred because they do not conflict. Every row is:

```toml
[[row]]
id = "header.assert.h"
status = "present"          # present or waived
evidence = "test:tests/posix/check-headers.sh"
notes = "P1: assert.h compiles under the POSIX feature macros"
```

Structured evidence is mandatory for `present`:

* `qtest:<case>` resolves to `tests/qtest/cases/<case>.toml`; the gate runs
  `cargo xtask qtest --case <case> --arch <arch>` with a 900-second outer timeout. A missing case,
  a failed boot or a skipped case rejects the claim.
* `test:<repo-relative-path>` must exist; the gate runs it through the runner declared in its
  shebang (for example `#!/bin/sh`) or directly when it is an executable binary, with a
  300-second timeout. A non-zero exit, missing file or timeout rejects the claim.
* `waived` requires the written owner waiver in `notes`; `evidence` may be `none`. The gate keeps
  the waiver visible in the row reason, and `GAPS.md` appends the waiver text.
* The row id must already exist in the derived profile, and the owner file must match the row's
  owner. Duplicate ids across files are rejected.

The seed applies structurally valid rows after its own derivation, so they land in `profile.toml`,
`traceability.toml` and `GAPS.md`; the gate re-applies them and executes the evidence. Missing or
failing evidence stays a failure, and the gate never turns a claim into a pass without running it.
The gate counts must not regress: a supported row whose evidence passes replaces its derived
failure, while a fake path, a missing qtest or a failing test stays a failure.

Current sample: `P1.toml` closes `header.assert.h` with
`test:tests/posix/check-headers.sh` (`sh tests/posix/check-headers.sh`).
