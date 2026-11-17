<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4 utility waiver review

Status: OBSERVED (P4a7). This is an independent second check of the P4a2/P4f utility table, not a
re-implementation. It covers the 38 unimplemented `util.*` rows.

Audited table: `origin/jerry/P4f` commit `0a717b17`, `docs/posix/P4-utility-gaps.md` plus the 31
rows in `docs/posix/evidence/P4-utility-gaps.toml`. Current `origin/master` `08d62125` does not
contain that table: all 38 rows are still `missing` with `evidence = "none"` in
`docs/posix/profile.toml`, and P4a2's report is `WORKING` with `Pushed: none`.

## Method

The check used the published Issue 8 pages, not memory:

* Utility index: <https://pubs.opengroup.org/onlinepubs/9799919799/idx/utilities.html> (fetched
  2026-11-17, HTTP 200). A name absent from this index is treated as removed, with one exception:
  `times` is a shell special built-in, not a utility-index entry.
* Per-utility page: `https://pubs.opengroup.org/onlinepubs/9799919799/utilities/<name>.html`
  (the 22 utility pages plus the `times` redirect were fetched, all HTTP 200). The `NAME` line and
  the SYNOPSIS option prefix were read: `( DEVELOPMENT )` + `[ XSI ]`, `[ CD ]`, `[ UP ]`,
  `[ XSI ]`, or no marker.
* `times.html` redirects to
  `https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html#times`, section
  2.15 Special Built-In Utilities; the SYNOPSIS is plain `times`, no optional marker.

## Verdict

**The 31 waived rows are classified correctly and all 7 required rows are correctly required.**
No waiver is wrong and no required row is missing from the current implementation split. The table
itself is sound. The defects found are in the split text and in one required row's executability:

1. The audit's "P4g / P4h split" section is stale and names the wrong tickets; the real split is
   P4a3, P4a4, P4a5 and P4a6.
2. `ulimit` is assigned to P4a3 but its acceptance path cannot pass: the ABI has no
   `getrlimit`/`setrlimit`, and P4a3 reports `util.ulimit` as `fail` with `evidence = "none"`.
   This is the one required row without a green executable path.
3. The audit and its waiver overlay are not on `master`; P4a2 must land them or the gate still
   counts all 38 rows as failures.
4. The index-absence rule needs the `times` exception written down, because that name is absent
   from the utility index yet required as a special built-in.

## Classification table

All rows are `pass` for the classification itself. "Correct" means the P4f class matches the
Issue 8 page.

| Utility | P4f class | Verified class | Exact Issue 8 evidence | Verdict |
|---|---|---|---|---|
| `c99` | removed | removed | utility index: not listed (`c17` is listed) | correct |
| `cod` | removed | removed | utility index: not listed | correct |
| `fort77` | removed | removed | utility index: not listed | correct |
| `neqn` | removed | removed | utility index: not listed | correct |
| `qalter` | removed | removed | utility index: not listed | correct |
| `qdel` | removed | removed | utility index: not listed | correct |
| `qhold` | removed | removed | utility index: not listed | correct |
| `qmove` | removed | removed | utility index: not listed | correct |
| `qmsg` | removed | removed | utility index: not listed | correct |
| `qrerun` | removed | removed | utility index: not listed | correct |
| `qrls` | removed | removed | utility index: not listed | correct |
| `qselect` | removed | removed | utility index: not listed | correct |
| `qsig` | removed | removed | utility index: not listed | correct |
| `qstat` | removed | removed | utility index: not listed | correct |
| `qsub` | removed | removed | utility index: not listed | correct |
| `admin` | optional | optional | `admin` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `delta` | optional | optional | `delta` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `get` | optional | optional | `get` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `prs` | optional | optional | `prs` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `rmdel` | optional | optional | `rmdel` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `sact` | optional | optional | `sact` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `sccs` | optional | optional | `sccs` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `unget` | optional | optional | `unget` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `val` | optional | optional | `val` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `what` | optional | optional | `what` page: `NAME` `(DEVELOPMENT)`, SYNOPSIS `[ XSI ]` | correct |
| `c17` | optional | optional | `c17` page: SYNOPSIS `[ CD ]` | correct |
| `ex` | optional | optional | `ex` page: SYNOPSIS `[ UP ]` | correct |
| `vi` | optional | optional | `vi` page: SYNOPSIS `[ UP ]` | correct |
| `ipcrm` | optional | optional | `ipcrm` page: SYNOPSIS `[ XSI ]` | correct |
| `ipcs` | optional | optional | `ipcs` page: SYNOPSIS `[ XSI ]` | correct |
| `tabs` | optional | optional | `tabs` page: SYNOPSIS `[ XSI ]` | correct |
| `localedef` | required | required | `localedef` page: SYNOPSIS has no option marker | correct |
| `lp` | required | required | `lp` page: SYNOPSIS has no option marker | correct |
| `newgrp` | required | required | `newgrp` page: SYNOPSIS has no option marker | correct |
| `time` | required | required | `time` page: SYNOPSIS has no option marker | correct |
| `times` | required | required | `times` redirects to 2.15 Special Built-In Utilities; no marker | correct |
| `tput` | required | required | `tput` page: SYNOPSIS has no option marker | correct |
| `ulimit` | required | required | `ulimit` page: base `ulimit [ -H|-S ] -a` has no marker; `[ XSI ]` is only on the `-c|-d|-f|-n|-s|-t|-v` form | correct |

Counts: 15 removed + 16 optional = 31 waived; 7 required. The index itself lacks 16 of the 38
names, but `times` is the special-built-in exception, so the removed count is 15.

## Required-row split

The current ticket set assigns every required row and gives it a foreground acceptance command.
None of the seven rows is present on `master` yet; the "observed" column is the owner report
status at review time.

| Required row | Assigned ticket | Executable acceptance path | Observed |
|---|---|---|---|
| `time` | P4a3 (bob, `bob/P4g` `c5c9acf0`) | `sh tests/posix/utility/run-simple2.sh`; `cargo nextest run -p time`; `cargo xtask posix-check --profile posix --arch x86_64 --host-only`; `make check-fast` | implemented, row pass on the branch; not merged |
| `times` | P4a3 (bob) | same as `time` with `-p times` | implemented, row pass on the branch; not merged |
| `tput` | P4a3 (bob) | same as `time` with `-p tput` | implemented, row pass on the branch; not merged |
| `ulimit` | P4a3 (bob) | same as `time` with `-p ulimit` | **red**: parser only, no `getrlimit`/`setrlimit`; `util.ulimit` fail / `none` |
| `localedef` | P4a4 (otto, `otto/P4h`) | `sh tests/posix/utility/run-simple3.sh`; `cargo nextest run -p localedef`; `posix-check --host-only`; `make check-fast` | 10 nextest + runner 1 pass; report WORKING, not merged |
| `newgrp` | P4a5 (mel, `mel/P4a5`) | `sh tests/posix/utility/run-simple4.sh`; `cargo nextest run -p newgrp`; `posix-check --host-only`; `make check-fast` | 6 nextest + runner pass; report WORKING, not merged |
| `lp` | P4a6 (tim, `tim/P4j`) | `sh tests/posix/utility/run-simple5.sh`; `cargo nextest run -p lp`; `posix-check --host-only`; `make check-fast` | 28 nextest + runner 1 pass; report WORKING, not merged |

No required row is unassigned. `ulimit` is the only assigned row whose path cannot currently turn
the gate row green: `crates/raven-abi/src/syscall.rs` has no `getrlimit`/`setrlimit`, and the
`raven-libc-platform` rlimit cell only has `RLIMIT_NOFILE`. P1f already records `ulimit` as
BLOCKED on `RLIMIT_FSIZE`; this needs an ABI/kernel resource-limit ticket before P4a3 can claim the
row. Owner: bob/P4a3 for the utility, kernel/ABI owner for the primitive.

## Corrections for P4a2 / Jerry

1. Replace the audit's "P4g / P4h split" section with the actual assignment: P4a3 covers `time`,
   `times`, `tput` and `ulimit` (plus the waived `tabs`, `unget`, `what`); P4a4 covers
   `localedef`; P4a5 covers `newgrp`; P4a6 covers `lp`. The old text's P4g is a different
   profile-registration ticket and P4h is the merged `sed` batch.
2. State the `times` exception in the Source section: an absent utility-index entry means removed
   except for shell special built-ins, where `times` is required by 2.15.
3. Land the audit and its 31-row waiver overlay. They exist only on `origin/jerry/P4f`
   `0a717b17`; `origin/master` `08d62125` still has all 38 rows `missing`/`none`. P4a2's report
   is `WORKING` with `Pushed: none`, so the gate cannot see the waivers yet.
4. Treat `ulimit` as a red required row until an executable path exists. The classification is
   correct; the implementation split is not yet satisfiable. Report the missing primitive to the
   kernel/ABI owner and keep the row `missing` rather than claiming a waiver.
5. Do not weaken any row to make the 31/7 split look complete: the 31 waivers are defensible, and
   the 7 required rows must stay required even while their tickets are in flight.

## Reproduction

```sh
# Table and evidence under review
git show 0a717b17:docs/posix/P4-utility-gaps.md
git show 0a717b17:docs/posix/evidence/P4-utility-gaps.toml

# Current master state
cargo xtask posix-gaps --check
cargo run -q -p posix-conform -- seed
```

The Issue 8 fetches were HTTP 200 for the index and for each of `admin c17 delta ex get ipcrm ipcs
localedef lp newgrp prs rmdel sact sccs tabs time tput ulimit unget val vi what`; `times` was
HTTP 200 and redirected to the shell chapter. No standard prose is copied in this note.
