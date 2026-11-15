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
cargo xtask posix-gaps            # regenerate GAPS.md
cargo xtask posix-gaps --check    # validate ids/owners/evidence/classification and freshness
cargo test -p posix-conform
```

The profile assumes the full Shell and Utilities volume. No requirement is silently omitted:
owner waivers, when decided, are explicit `status = "waived"` rows with a written `waiver`.
