<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc world gate 2.1/2.2: supply or waive

Date: 2026-11-18. Status: **owner decision required**. Owner: Bo Maryniuk.
Scope: libc gate rows 2.1 (world check) and 2.2 (world build gate) for the 0.1.0 release.

## Decision summary

Rows 2.1/2.2 cannot be evaluated from the tree as it stands: the H-11/H-12 world inputs are
absent and all 200 world rows are placeholders. The owner must either supply the frozen
tarballs/lock (Option A) or record an explicit 0.1.0 waiver (Option B). Neither option edits a
gate row or fabricates `world.json`.

## Evidence (OBSERVED, C74bu, 2026-11-18)

* `/home/bo/work/ravenos-workspace/ravenbsd-resources` exists with the donor checkouts, but has
  **no `world-src/`**.
* `donors.lock` has **no world/tarball entry** (`grep -i world` is empty).
* `libc/ravenlibc/world.toml`: **200 packages, 200 × `version = "PLACEHOLDER"`,
  `freeze = "pending"`**.
* Smoke5 sources are not present as pristine tarballs: zlib only under `netbsd/external/zlib`,
  lua under `netbsd/lib/lua` and `openzfs/module/lua`; sqlite, pcre2 and toybox are absent.
* `cargo run -q -p libc-world -- build --subset smoke5` (with rustc's `gcc-ld/ld.lld` on PATH):
  **5 packages, 0 built, 5 placeholder**. The command exits 0 because the run completed, not
  because anything built.
* `cargo run -q -p libc-gate -- eval --only 2.1` / `2.2`:
  **BLOCKED**, reason `missing tools/libc-world/results/x86_64/world.json` and H-11 tarballs
  absent.
* Tooling is healthy: `cargo nextest run -p libc-world` is **279/279 green**; `make check-fast`
  is green.

## Option A - supply the frozen world inputs

Owner supplies the H-11 tarballs and H-12 freeze; the build then produces real results.

Required inputs:

1. `ravenbsd-resources/world-src/` tarballs whose names match the `tarball` fields in
   `libc/ravenlibc/world.toml` (at minimum the smoke5: zlib, lua, sqlite, pcre2, toybox).
2. A `donors.lock` entry per tarball: name, upstream URL/revision and SHA-256.
3. Real `version`/`url`/`sha256`/`tarball` values in `world.toml` and `freeze = "frozen"`.
4. `tools/libc-world/results/x86_64/world.json` and the per-package JSON/undef/header evidence
   produced by the real build.

Commands once the inputs exist (from the repository root; `R` = the resources directory):

```
cargo run -q -p libc-world -- check --strict --tarballs "$R/world-src"
PATH="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld:$PATH" \
  cargo run -q -p libc-world -- build --subset smoke5 \
    --tarballs "$R/world-src" --sysroot build/x86_64/sysroot
cargo run -q -p libc-world -- report
cargo run -q -p libc-world -- missing --resources "$R"
cargo run -q -p libc-gate -- eval --only 2.1 --out build/libc-gate-c74db
cargo run -q -p libc-gate -- eval --only 2.2 --out build/libc-gate-c74db
make check-fast
```

Use `--all` instead of `--subset smoke5` for the full 200-row run once smoke5 is clean. The
`gcc-ld` PATH entry is only a host-tool alias for `ld.lld`; installing `lld` also works.

Release consequence: 2.1/2.2 can be evaluated and may become MET; the release gate then includes
a real third-party world build. Cost: the tarballs, lock entries and the freeze are owner inputs;
build time and failures are real. No waiver is needed.

## Option B - explicit 0.1.0 waiver (no gate-row edit)

The owner records in the release decision/notes that 2.1/2.2 are deferred for 0.1.0 because the
H-11/H-12 inputs are not available. The gate rows stay **BLOCKED** in
`tests/libc/gate/rows.toml`; they are not edited to MET and no placeholder is counted as built.

Suggested waiver text:

> libc world rows 2.1/2.2 are waived for the 0.1.0 release: the H-11 pristine tarballs and the
> H-12 world freeze (`world.toml`, `donors.lock`) are not supplied. They remain open for the next
> release; no `world.json` is claimed.

Release consequence: 0.1.0 ships without a third-party world-build proof. The libc gate verdict
stays NOT MET for row 2; the release must explicitly accept that documented waiver. Risk:
third-party build compatibility is unmeasured until the inputs land. No fabricated results and
no change to the frozen libc ABI or the other gate rows.

## Recommendation

If the frozen tarballs can be supplied before the 0.1.0 cut, take Option A; it is the only path
that can make 2.1/2.2 MET. If they cannot, take Option B explicitly in the release notes rather
than leaving the rows silently blocked or counting placeholders.

Owner decision: **Option A (supply)** / **Option B (waive)** - signed/date: ____________________
