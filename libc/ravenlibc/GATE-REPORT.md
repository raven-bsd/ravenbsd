<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ravenlibc libc gate report

Evaluated by `libc-gate` on 2026-11-17 from the C74b tree (origin/master `1eed3518`). Every
status below has an evidence file under `build/libc-gate/`; the table is
`cargo run -q -p libc-gate -- table`. C74b closes the `libc-lint` row and makes `libc-pack`
stage `build/<arch>/sysroot` repeatably; the remaining NOT MET rows are outside C74b's Owns and
are listed with their owning lane.

## 1. Gate table

| Row | Check | Status | Evidence | Note |
|---|---|---|---|---|
| 1 correctness | 1.1 symbol coverage (C1) | MET | build/libc-gate/1.1.txt | exit 0 |
| 1 correctness | 1.1.pack packed symbol list (C1) | MET | build/libc-gate/1.1.pack.txt | exit 0 |
| 1 correctness | 1.1.audit target-case symbol audit (C64j) | NOT MET | build/libc-gate/1.1.audit.txt | exit 1: expected 0 |
| 1 correctness | 1.2 donor suites G1 (C2-C6) | NOT MET | build/libc-gate/1.2.txt | exit 3: expected 0 |
| 1 correctness | 1.3 LTP and pjdfstest (C7, C8) | INFO | build/libc-gate/1.3.txt | owner-run LTP/pjdfstest results not supplied; INFO per C71/3.B.1.4 |
| 1 correctness | 1.4 differential tests (C9) | NOT MET | build/libc-gate/1.4.txt | 27 oracle families |
| 1 correctness | 1.5 host unit tests | MET | build/libc-gate/1.5.txt | 31 host crates |
| 1 correctness | 1.6 Miri (C11) | BLOCKED | build/libc-gate/1.6.txt | preflight command failed |
| 1 correctness | 1.7 lints (C16) | MET | build/libc-gate/1.7.txt | exit 0 |
| 2 portability | 2.1 world check | BLOCKED | build/libc-gate/2.1.txt | missing tools/libc-world/Cargo.toml; C72 outputs are not in origin/master and H-11 tarballs are absent |
| 2 portability | 2.2 world build gate (C13) | BLOCKED | build/libc-gate/2.2.txt | missing tools/libc-world/Cargo.toml; C72 outputs are not in origin/master and H-11 tarballs are absent |
| 3 size | 3.1 size budgets (C14) | NOT MET | build/libc-gate/3.1.txt | exit 1: expected 0 |
| 3 size | 3.2 boot path (C15) | INFO | build/libc-gate/3.2.txt | PG-60 absent; 0-1 syscalls and <=250 instructions are not evaluable |
| 4 hardening | 4.1 fuzz replay | MET | build/libc-gate/4.1.txt | exit 0 |
| 4 hardening | 4.2 fuzz soak (C10) | NOT MET | build/libc-gate/4.2.txt | inet_text has no soak duration; inet_text has 1 crash(es); inet_text has no executions |
| 4 hardening | 4.3 hardened malloc default | MET | build/libc-gate/4.3.txt | hardened backend selected; tiny not in libc.so.1 |
| 4 hardening | 4.4 ENTROPY_WEAK status (H-5) | INFO | build/libc-gate/4.4.txt | ENTROPY_WEAK CLEAR; PG-52 release blocker, not a libc-gate blocker |
| 5 on-target | 5.1 on-target libc cases (C12) | NOT MET | build/libc-gate/5.1.txt | 7 qtest cases |
| 5 on-target | 5.2.core-servers core-servers regression | NOT MET | build/libc-gate/5.2.core-servers.txt | exit 1: expected 0 |
| 5 on-target | 5.2.file-tools file-tools regression | NOT MET | build/libc-gate/5.2.file-tools.txt | exit 1: expected 0 |
| 5 on-target | 5.3 second architecture | INFO | build/libc-gate/5.3.txt | aarch64: not delivered (C73 open item); owner decides whether G1 closes on x86_64 alone |
| 6 documentation | 6.1 page coverage | MET | build/libc-gate/6.1.txt | exit 0 |
| 6 documentation | 6.2 render lint | BLOCKED | build/libc-gate/6.2.txt | preflight command failed |
| 6 documentation | 6.3 existing docs gate | MET | build/libc-gate/6.3.txt | exit 0 |
| 6 documentation | 6.4 libc(7) | MET | build/libc-gate/6.4.txt | docs/man7/libc.7 exists |

Verdict: NOT MET

## 2. Verdict

**libc gate: NOT MET.**

Rows 1 (correctness), 3 (size), 4 (hardening) and 5 (on-target) are NOT MET. Rows 2
(portability) and 6 (documentation) are BLOCKED. MET checks: symbol coverage and the packed
symbol list, the 31 host unit-test crates, fuzz replay, the hardened allocator default, page
coverage, the existing docs gate and `libc(7)`. The gate cannot close.

C57c and C76 are merged, so the old `libc-threads` list-corruption blocker is gone. The thread
case now advances through `mutex/cond/once/tsd/tls/rwlock/sem/barrier` and times out before
`detach` returns. The remaining on-target gaps are unchanged in kind and are listed in section 4.

## 3. Rows changed by this refresh

* 1.1.audit: still NOT MET. `cargo xtask libc-audit` reports 66 required, 66 present, 0 missing,
  but the `tests/libc/symbol-audit/expected.tsv` snapshot differs from the generated matrix; the
  snapshot is stale (C64j/C64l follow-up).
* 1.2 G1: now NOT MET rather than BLOCKED. `raven-atf-run gate --g1` exits 3: C2-C5 are 0 %
  (not run) and C6 is unvendored; 0 of 4 measured rows (C71a/C71).
* 1.4: 23 of 27 oracle families pass; `compat-crypt`, `compat-humanize`, `compat-setmode` and
  `compat-vis` fail because `raven-libc-oracle` has no `oracle-<family>` feature (C35/C71).
* 1.7: now MET, exit 0. The one `pthread/sem_trywait.rs` missing-safety violation was fixed by
  adding the `// SAFETY:` comment in C74b.
* 5.1: only `libc-signals` and `libc-spawn` pass; section 4 has the exact case output.
* 5.2: both inherited qtests now fail with timeouts. `core-servers` times out after 120 s and
  `file-tools` after 240 s; both serial logs stop at `loader: RavenBSD loader 0.1 (x86_64)` and
  never reach `init`. Both were MET on the previous base, so this is a regression on the merged
  master that needs the kernel/server lane; C74a does not edit kernel or procd.

## 4. Seven-case matrix (OBSERVED, 2026-11-17)

Each row is a direct `cargo xtask qtest --case <case> --arch x86_64` run.

| Case | Result | Evidence and exact gap |
|---|---|---|
| libc-hello | FAIL | `libc-hello: FAIL auxv` (line 191) and `FAIL string` (line 195); the auxv check is the PG-50 startup block, the string check is the shell string routines. |
| libc-stdio | FAIL | `libc-stdio: printf ok`, then `process: child killed by page fault at 0x3`; the first stdio file/stream check after printf. |
| libc-time | ERROR (build) | The C fixture does not link: `rust-lld: undefined symbol: usleep`, `sleep`, `tzname`. These are genuinely absent from `libc.a` (`nm` shows only `nanosleep`); `sleep`/`usleep` are unistd and `tzname` is time (P1b), all outside C74b's Owns. C74b makes the sysroot repeatable but does not add these symbols. |
| libc-threads | FAIL (timeout) | Last line `libc-threads: barrier ok`; `detach`, `stack`, `c11`, `exit value`, `timedwait` and `done` never print; the 300 s case timeout fires and `svcd: settled` / `init exited (0)` are absent. |
| libc-signals | PASS | `cargo xtask qtest --case libc-signals --arch x86_64` exit 0. |
| libc-dynamic | ERROR (build) | `shape: FAIL partial file tail at 0x0000000000000000 (offset 0x000000 + size 0x000ae4)`. The loader shape check rejects the produced file. |
| libc-spawn | PASS | `cargo xtask qtest --case libc-spawn --arch x86_64` exit 0. |

Supporting acceptance:

```
$ cargo xtask libc-lint
libc-lint: ok (3989 files scanned)
exit 0

$ cargo run -q -p raven-atf-run -- gate --g1
G1: NOT MET (0 of 4 measured rows)
exit 3
```

C74b also made `cargo xtask libc-pack` stage `build/<arch>/sysroot` (crt objects, `libc.a`, stub
archives, base/arch/generated headers, and `libc.so.1`/`ld-elf.so.1` when produced by
`libc-link`) and write the generated `Symbols.list` beside the archive instead of dirtying
`libc/ravenlibc/abi/Symbols.list`. This removes the manual sysroot staging step; it does not
change the missing `sleep`/`usleep`/`tzname` exports or the thread/dynamic/hello failures, which
live in crates and linker paths outside C74b's Owns.

## 5. Other rows

* 1.3 LTP/pjdfstest: INFO, no owner-supplied runs.
* 1.6 Miri: BLOCKED, `build/libc-gate/1.6.txt`; the Miri preflight fails because the miri
  component is not installed.
* 2.1/2.2: BLOCKED, `tools/libc-world/Cargo.toml`, C72 outputs and H-11 tarballs are absent.
* 3.1: NOT MET, `libc-size check: 7 pass, 62 fail, 68 known, 102 skipped, 0 budget-only`; the
  failures include P0/P1/P4/P6 size, `INV-LIBC-SO-NEEDED` on all three arches, boot instruction
  limits and `libc.so.1` size.
* 4.2: NOT MET, the `inet_text` soak has no duration, one crash and no executions.
* 6.2: BLOCKED, `which mandoc` exits 1.
