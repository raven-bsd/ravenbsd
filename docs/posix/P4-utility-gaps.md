<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4f - POSIX.1-2024 utility requirement audit

Status: the 38 unimplemented P4e utility rows are classified against the Open Group Base
Specifications Issue 8, IEEE Std 1003.1-2024. **31 rows are waived** (15 removed, 16 optional) and
**7 required rows stay missing** with a named minimal implementation. The profile and its
generated files are refreshed from this audit; no utility code is changed.

## Source

The classification uses the published Issue 8 utility pages:

* Utility index: <https://pubs.opengroup.org/onlinepubs/9799919799/idx/utilities.html> — a utility
  absent from this list is **removed** in POSIX.1-2024.
* Utility page: `https://pubs.opengroup.org/onlinepubs/9799919799/utilities/<name>.html` — the
  SYNOPSIS prefix carries the option marker:
  * `( DEVELOPMENT )` with `[ XSI ]` — optional Software Development Utilities;
  * `[ CD ]` — optional C-Language Development Utilities;
  * `[ UP ]` — optional User Portability Utilities;
  * `[ XSI ]` — optional XSI;
  * no marker — required base utility (a per-option `[ XSI ]` line only makes that option form
    optional, not the utility).

The exact marker is quoted per row below. No standard prose is copied.

## Removed in Issue 8 — waived (15)

These names are absent from the Issue 8 utility index; they are explicit `waived` profile rows.

| Utility | Class | Exact source | Action |
|---|---|---|---|
| `c99` | removed | Issue 8 utility index: not listed; `c17` is the C compiler | waived |
| `cod` | removed | Issue 8 utility index: not listed | waived |
| `fort77` | removed | Issue 8 utility index: not listed (FORTRAN dropped) | waived |
| `neqn` | removed | Issue 8 utility index: not listed | waived |
| `qalter` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qdel` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qhold` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qmove` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qmsg` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qrerun` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qrls` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qselect` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qsig` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qstat` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |
| `qsub` | removed | Issue 8 utility index: not listed (Batch Environment Services) | waived |

## Optional in Issue 8 — waived (16)

These rows are in Issue 8 only inside an optional feature group; the implementation does not claim
that group.

| Utility | Class | Exact source | Action |
|---|---|---|---|
| `admin` | optional | Issue 8 `admin` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `delta` | optional | Issue 8 `delta` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `get` | optional | Issue 8 `get` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `prs` | optional | Issue 8 `prs` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `rmdel` | optional | Issue 8 `rmdel` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `sact` | optional | Issue 8 `sact` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `sccs` | optional | Issue 8 `sccs` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `unget` | optional | Issue 8 `unget` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `val` | optional | Issue 8 `val` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `what` | optional | Issue 8 `what` page: `( DEVELOPMENT )`, `[ XSI ]` | waived |
| `c17` | optional | Issue 8 `c17` page: `[ CD ]` (C-Language Development Utilities) | waived |
| `ex` | optional | Issue 8 `ex` page: `[ UP ]` (User Portability Utilities) | waived |
| `vi` | optional | Issue 8 `vi` page: `[ UP ]` (User Portability Utilities) | waived |
| `ipcrm` | optional | Issue 8 `ipcrm` page: `[ XSI ]` | waived |
| `ipcs` | optional | Issue 8 `ipcs` page: `[ XSI ]` | waived |
| `tabs` | optional | Issue 8 `tabs` page: `[ XSI ]`; removed from User Portability in Issue 7 | waived |

## Required in Issue 8 — missing (7)

These rows have no option marker in the Issue 8 SYNOPSIS and remain `missing`; the minimal
implementation scope is named for the P4g/P4h work.

| Utility | Exact source | Minimal implementation |
|---|---|---|
| `localedef` | Issue 8 `localedef` page: required (no option marker) | parse the POSIX locale definition and charmap and emit the runtime catalog format already consumed by raven-libc-locale; no new kernel primitive |
| `lp` | Issue 8 `lp` page: required (no option marker) | POSIX `lp` client for the print service, with the `-c -d -n -m -s -w -o -t` options and the specified no-destination diagnostic; a hardcopy device is not required |
| `newgrp` | Issue 8 `newgrp` page: required (no option marker) | group-database lookup plus the credential/exec path to start a new shell with the new real/effective gid; `-l` and the no-operand form |
| `time` | Issue 8 `time` page: required (no option marker) | POSIX `-p` timing over `clock_get`/`times`, statistics to stderr, child exit status propagation |
| `times` | Issue 8 shell special built-in (the `times` page redirects to 2.15) | shell special built-in using the existing process/child times; declared by the shell feature rows |
| `tput` | Issue 8 `tput` page: required (no option marker) | terminfo/termios subset for the required capabilities; unsupported terminal returns a diagnostic and the specified exit status |
| `ulimit` | Issue 8 `ulimit` page: required base `-a` form (the `-c/-d/-f/-n/-s/-t/-v` form is `[ XSI ]`) | base `-a` report/set over the existing rlimit syscalls; the XSI option form is optional |

## P4g / P4h split

* **P4g (existing ticket)** covers `time`, `times`, `tput`, `tabs`, `ulimit`, `unget`, `what`.
  After this audit only `time`, `times`, `tput` and `ulimit` are required; `tabs`, `unget` and
  `what` are optional and are waived here, so P4g's required scope is four tools.
* **P4h (remaining required batch)** covers `localedef`, `lp` and `newgrp`. These need locale,
  print-service and credential/session work respectively and are too large for the P4g simple-tool
  batch. The existing P4h ticket is the already-implemented `sed` batch; the lead should re-scope
  P4h (or assign a new id) to this three-tool batch.
* The 31 waived rows need no implementation; they stay visible in `GAPS.md` with their written
  waiver.

## Generated files

`cargo run -q -p posix-conform -- seed` regenerates `docs/posix/profile.toml`,
`docs/posix/traceability.toml` and `docs/posix/GAPS.md` from the seed plus the evidence overlays.
This audit adds `docs/posix/evidence/P4-utility-gaps.toml` with the 31 `waived` rows; the seven
required rows are untouched and remain `missing`. No utility, shell or libc code is part of P4f.
