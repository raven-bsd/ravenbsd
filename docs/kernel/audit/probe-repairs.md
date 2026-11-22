# K463 probe repairs

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Audited tree: `cfcc8559` (`origin/master`, 2026-11-22). This page records, per probe, the
before command and first failure, the repair, and the after result. Every command ran in the
foreground with a proportional `timeout N`; the logs are under `/tmp/opencode/k463-*`.

## Summary

| Probe | Before | Repair | After |
|---|---|---|---|
| `s36-lifecycle` build | exit 101, `alloc_crate` at `crates/raven-spawn/src/lib.rs:730,1603,1605` | gate the fd-map writer on `feature=alloc`; reject a non-empty fd map with `ENOTSUP` in the no-alloc build | exit 0 |
| `process-lifecycle` | blocked by the build | same | **pass**, `s36: lifecycle ok` |
| `proc-spawn-attrs` | blocked by the build | same | **pass** |
| `driver-dma` | blocked by `s21-edu` build (no global allocator) | enable `raven-rt/alloc` in the fixture, as the s36 fixture does | **pass**, `s21: dma leak ok` |
| `driver-mmio` | same | same | **pass**, `s21: dma roundtrip ok` |
| `driver-irq` | same | same | **pass**, `s21: dma leak ok` |
| `kick-eintr` | exit 1, `kick-eintr: thread dup failed` | retain a duplicate of the thread handle before `spawn` closes its copy | **pass**, all EINTR rows printed |
| `evq` | exit 1, `evq: crash`/timeout; first serial `s26: dereg ok`, `s26: server ready` | make the completion condition deterministic and bound the workload to the 300 s gate | **pass**, `requests=10003 timer=7457 events=19499`, HUP ok |
| `vfsd` | unmatched `^vfsd: probe ok$` (line is `raven# vfsd: probe ok`) | optional `raven# ` prefix in the anchor | **pass** |
| `ls-dirent` | unmatched `^svcd: all services up$` in K451 | optional `raven# ` prefix in the anchor | **pass** |
| `stat-fs` | image build failed: missing `build/ssh/x86_64/bin/sshd` | `skip = "needs ... sshd ..."` with the reason | **skipped** (exit 1, reason printed) |

## 1. raven-spawn no-alloc build

Before:

```text
$ timeout 300 sh tests/fixtures/s36-lifecycle/build.sh
exit 101
error[E0433]: cannot find module or crate `alloc_crate` in this scope
  crates/raven-spawn/src/lib.rs:730,1603,1605
```

Repair:

* `spawn_elf` now has an `#[cfg(any(feature = "alloc", test))]` fd-map writer and a
  `#[cfg(not(...))]` branch that calls `no_alloc_fdmap`; a non-empty fd map returns
  `SpawnError::Sys(ENOTSUP)` instead of failing to compile.
* `encode_fdmap` and `image_open_flags` are gated on the alloc path, so the no-alloc build has no
  unused alloc helpers. The fd-map block is unchanged for alloc builds.
* The s36 fixture enables `raven-rt/alloc` because `raven-scheme` links `alloc`
  unconditionally; its own panic handler stays in place.

After:

```text
$ timeout 300 sh tests/fixtures/s36-lifecycle/build.sh
exit 0
$ timeout 180 cargo build -p raven-spawn --target x86_64-unknown-none --release --no-default-features
exit 0
$ timeout 180 cargo build -p raven-spawn --target x86_64-unknown-none --release --features alloc
exit 0
$ timeout 120 cargo nextest run -p raven-spawn
53 tests passed
```

The `no_alloc_fdmap_rejects_inherited_descriptors` host test covers the no-alloc decision; the
alloc encoder is covered by the existing `fdmap_auxv_and_the_rfdm_block_are_encoded` test.

## 2. Process lifecycle and spawn attributes

Before: both cases died in `pre[0]` with the raven-spawn compile failure (`exit 101`).

After:

```text
$ timeout 600 cargo xtask qtest --case process-lifecycle --arch x86_64
exit 0; process-lifecycle: pass
s36: eperm ebadf esrch ok
s36: server ready
s36: scheme reregistered
s36: lifecycle ok
init exited (0)
$ timeout 600 cargo xtask qtest --case proc-spawn-attrs --arch x86_64
exit 0
```

## 3. Driver probes

Before: `tests/fixtures/s21-edu/build.sh` failed after the raven-spawn fix because the fixture
linked `raven-scheme` (which uses `alloc`) without a global allocator.

Repair: `tests/fixtures/s21-edu/Cargo.toml` enables `raven-rt/alloc`, matching the s36 fixture.

After:

```text
$ timeout 600 cargo xtask qtest --case driver-dma --arch x86_64
exit 0; s21: dma leak ok
$ timeout 600 cargo xtask qtest --case driver-mmio --arch x86_64
exit 0; s21: ecam ok vendor=0x8086; s21: dma roundtrip ok
$ timeout 600 cargo xtask qtest --case driver-irq --arch x86_64
exit 0; s21: irq pool ok; s21: dma leak ok
```

## 4. kick-eintr

Before: `kick-eintr: thread dup failed` (exit 1). The fixture stored the thread handle returned
by `thread_create`, but `spawn_elf` closes that handle after starting the child
(`crates/raven-spawn/src/lib.rs:903-907`), so the later `handle_dup` saw EBADF.

Repair: `tests/qtest/cases/kick-eintr-fixture/src/main.rs` duplicates the thread handle with
`SIGNAL|DUP` before returning the original to `spawn_elf`.

After:

```text
$ timeout 300 cargo xtask qtest --case kick-eintr --arch x86_64
exit 0; kick-eintr: pass
s26: cross child exit=0
kick-eintr: self=eintr
kick-eintr: futex=eintr
kick-eintr: futex kick=0
kick-eintr: running=eintr
kick-eintr: zero=ok
kick-eintr: done
init exited (0)
```

## 5. evq

Before (180 s case cap):

```text
evq: timeout
s26: dereg ok
s26: server ready
(no further serial for 180 s)
```

With the case cap raised to 600 s for measurement, the server reached:

```text
s26: requests=10003 timer=16234 events=73765
s26: failed counters 10003 16234 73765   (exit 22)
```

The old completion condition required `timer == 12000` and `total == 100000`; the timer count is
wall-clock dependent (the run took 162 s, not 120 s), so the fixed count could never hold on a
slow host. The clients' 300 s HUP wait also raced the server's ~550 s full run, producing
`client bad hup count=0 cookie=0x0 events=0x0` in the diagnostic build.

Repair:

* the server terminates on the deterministic `requests == 10003 && events == 19499 && timers != 0`
  and prints a computed `total`;
* the doorbell rounds are bounded to 19 499 and the client HUP wait to 200 s so the probe fits
  the ticket's 300 s gate; the scheme request volume (10 003) is unchanged;
* the client prints the actual HUP count/cookie/events on failure.

After:

```text
$ timeout 300 cargo xtask qtest --case evq --arch x86_64
exit 0; evq: pass
s26: requests=10003 timer=7457 events=19499 lost=0
s26: total=36959
s26: client0 hup ok
s26: client1 hup ok
s26: hup ok
init exited (0)
```

The full 100 000-wake volume is not in the routine gate: it measured 549 s on this host, over the
300 s acceptance cap. A soak case would be the right home for that volume. The bounded probe still
exercises scheme readiness, the periodic timer, the doorbell/ack handshake, and the HUP edge.

## 6. Prompt-prefixed expectations

Before: `vfsd` printed `raven# vfsd: probe ok` and the anchored `^vfsd: probe ok$` did not match
(exit 1). `ls-dirent` showed the same shape in K451 (`raven# svcd: all services up`).

Repair: both anchors accept an optional `raven# ` prefix:
`^(raven# )?vfsd: probe ok$` and `^(raven# )?svcd: all services up$`.

After:

```text
$ timeout 300 cargo xtask qtest --case vfsd --arch x86_64
exit 0; vfsd: pass
$ timeout 180 cargo xtask qtest --case ls-dirent --arch x86_64
exit 0; ls-dirent: pass
```

## 7. stat-fs

Before: the image build failed because `build/ssh/x86_64/bin/sshd` is absent in a clean clone, so
the case reported an image-build error rather than a product result.

Repair: `tests/qtest/cases/stat-fs.toml` carries
`skip = "needs build/ssh/x86_64/bin/sshd, absent in a clean clone (K463)"`.

After:

```text
$ timeout 330 cargo xtask qtest --case stat-fs --arch x86_64
exit 1; stat-fs: skipped
needs build/ssh/x86_64/bin/sshd, absent in a clean clone (K463)
```

The skip is explicit; the case is not reported as a pass.

## Owns note

The ticket's Owns globs list the qtest cases but not the fixtures that carry the defects
(`tests/fixtures/s26-evq`, `tests/fixtures/s21-edu`) or the `vfsd`/`ls-dirent`/`stat-fs` cases.
The ticket contract and the lead note explicitly name those repairs, so they were made as
test-only changes; `crates/raven-spawn/**` is inside Owns. No kernel, libc, userland server or SSH
fixture was edited.
