<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Dependency dashboard

Status: first SC3 weekly run, 2026-11-26 on `lance/SC3`. Policy: DR-64 and
[`supply-chain.md`](supply-chain.md); operational runbook:
`ravenbsd-prjmgmt/process/SUPPLY-CHAIN.md` section 5. Machine-readable data:
[`../metrics/deps.json`](../metrics/deps.json); produce it with
`sh scripts/deps-audit.sh --write` and gate it with `--check`.

The loop is a **decision queue, not an alarm list**. Every unmaintained or
duplicated item gets a remove/replace/fork/keep-pinned decision, an owner and a
review date. No dependency update is auto-merged: the weekly run opens a branch
or a report and the normal gates decide.

## 1. Dashboard

| Metric | Value | Source |
|---|---:|---|
| Workspace members | 504 | `cargo metadata` |
| Shipped third-party crates | 63 | resolve graph, `normal`+`build` edges |
| Direct third-party dependencies | 15 | member dependency lists |
| Transitive third-party crates | 48 | shipped minus direct |
| Optional direct third-party crates | 3 | member dependency lists |
| Dev-only third-party crates | 1 | `dev`-only edges |
| Locked third-party packages | 64 | `cargo metadata` packages minus members |
| Duplicate package names (shipped) | 1 (`bitflags` 1.3.2 + 2.13.2) | resolve graph |
| Unignored advisories | **0** | `cargo audit --json` (579 lockfile deps, 1568 advisories) |
| Reachable critical/high in shipped binaries | **0** | no advisory in the shipped graph |
| Reviewed advisory exceptions | 0 | `audit.toml` (`ignore = []`) |
| Unmaintained crates without a decision | 0 | `cargo audit` warnings empty |
| Vendor coverage | 771 crates (SBOM `1f89f0ed…`) | `vendor/manifest.toml` |
| Stale forks | 0 (next review 2027-02-15) | [`forks.md`](forks.md) |
| Time-to-remediate p50/p90 | n/a (0 real advisories patched to date) | audit history |

The five former false positives (`RUSTSEC-2018-0002`, `-2021-0080`, `-2026-0067`,
`-2026-0068`, `-2020-0071`) are **resolved**, not ignored: SC2 renamed the
first-party utilities to `raven-tar`/`raven-time`, and `audit.toml` now has an
empty ignore list.

## 2. Ranked decision queue

| Rank | ID / item | Kind | State | Decision | Owner | Review |
|---:|---|---|---|---|---|---|
| 1 | RUSTSEC-2018-0002 | first-party tar name collision | resolved | remove (SC2 `3fcb1ea9`) | kevin | 2026-12-05 |
| 2 | RUSTSEC-2021-0080 | first-party tar name collision | resolved | remove (SC2) | kevin | 2026-12-05 |
| 3 | RUSTSEC-2026-0067 | first-party tar name collision | resolved | remove (SC2) | kevin | 2026-12-05 |
| 4 | RUSTSEC-2026-0068 | first-party tar name collision | resolved | remove (SC2) | kevin | 2026-12-05 |
| 5 | RUSTSEC-2020-0071 | first-party time name collision | resolved | remove (SC2) | kevin | 2026-12-05 |
| 6 | `fork:toolchain/vendor/libc` | K158 pthread-ABI model | open | keep-pinned + ABI check | kevin | 2027-02-15 |
| 7 | `fork:userland/uutils` | coreutils `cf030b82…` | open | keep-pinned, quarterly sync | kevin | 2027-02-15 |
| 8 | `fork:userland/brush` | brush `8adb6bc0…` | open | keep-pinned, quarterly sync | kevin | 2027-02-15 |
| 9 | `frozen:crates/raven-rmm` | redox rmm `015af335…` | open | keep-pinned, replace-on-touch | mark | 2027-02-15 |

`bitflags` 1.3.2 / 2.13.2 is the only duplicate name in the shipped graph. It is
tracked, not urgent: the two versions come from different dependency ranges in
vendored graphs; a future batch update may collapse them.

## 3. Weekly loop

1. `sh scripts/deps-audit.sh --write` regenerates `metrics/deps.json`
   (`cargo metadata`, `cargo tree`, `cargo audit --json`, `cargo deny check
   advisories`, vendor manifest). The source commit, advisory DB revision and
   tool availability are recorded.
2. Review the diff in `counts`, `duplicates`, `advisories` and `queue`. A new
   advisory or a stale dashboard is a finding; run `--check` to let the gate
   fail on it.
3. For each new item, record a decision (remove/replace/fork/keep-pinned), the
   owner and a review date in this dashboard and in the script's queue seed.
4. For proposed patches: open a branch/report, run
   `cargo audit`, `sh scripts/deps-audit.sh --check`, the package tests and
   `make check-fast`. No auto-merge; the lead reviews.
5. Update [`forks.md`](forks.md) on any fork sync or exit and keep the
   `next review` dates current.

### Tool availability

`cargo-audit` 0.22.1 and `cargo-deny` 0.22.0 are available; `cargo deny check
advisories` reports `advisories ok`. `osv-scanner` is not installed, and
`cargo-udeps` 0.1.55 requires a nightly-only compiler flag that the pinned stable
toolchain rejects. `metrics/deps.json` records each status and reason, and they
are never counted as a pass. The dead/unmaintained coverage uses the `cargo
audit` unmaintained warnings plus the decision queue until OSV and a stable
dead-crate checker are wired into the build host.

## 4. Security hotfix lane

A critical/high advisory in a **shipped, reachable** path blocks the release and
may bypass a feature freeze, but never the security tests:

1. Triage: shipped vs dev-only, reachable function, exposure; record the
   advisory ID and the exact package/version.
2. Branch and fix in order: upgrade -> upstream patch -> local fork patch ->
   disable/replace. A fork patch must keep the register row and an ABI/behaviour
   check.
3. Run the targeted package tests, the libc/pf gates touched by the change, and
   `make check-fast`; the security-relevant tests are mandatory.
4. Owner approval is required to bypass a feature freeze; it is recorded with
   the advisory, the reachability evidence and an expiry.
5. After the patch: full soak for the affected area, a fresh release image and
   an updated `metrics/deps.json`; record the advisory-to-patch time for the
   p50/p90 metric.
6. Only if no fix exists: an owner-approved, expiring exception in `audit.toml`/
   `deny.toml` with ID, reason, reachability and review date. No blanket ignore.
