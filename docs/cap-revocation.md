<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Capability revocation

Contract C17: `HANDLE_REVOKE(h, mask)` removes `mask` from `h` and every handle derived from
it, including handles in other processes. The target handle stays open with its remaining
rights. A later operation that needs a revoked right fails `EPERM`; a right that was **never
granted** fails `EACCES`; a stale handle fails `EBADF`.

## Sequence

The `cap-revoke` guest case runs this sequence (`profile = base`,
`tests/fixtures/cap-revoke`):

| Step | Actor | Operation | Expected |
|---|---|---|---|
| 1 | controller | `MEMOBJ_CREATE`, then `HANDLE_TRANSFER` handle 3 (`MAP\|READ\|DUP`) and handle 4 (`READ\|DUP`) | both installs succeed |
| 2 | controller | `PROC_FORK(client)` then start the original and the clone | both processes hold the same lineage |
| 3 | client | `MAP` handle 3; `HANDLE_DUP` handle 3 before the revoke | `MAP` succeeds; the duplicate is a descendant |
| 4 | client | `MAP` handle 4 | `EACCES` (it never had `MAP`) |
| 5 | controller | `HANDLE_REVOKE(object, MAP)` | success |
| 6 | client | `MAP` handle 3, `MAP` the pre-revoke duplicate | `EPERM` in both processes |
| 7 | client | `HANDLE_DUP` handle 3 after the revoke, then `MAP` it | dup succeeds; `MAP` is `EACCES` (a post-revoke node has only `READ`) |
| 8 | client | `MAP` handle 4 and its duplicate | `EACCES` (never granted); both processes pass after the walk fix |
| 9 | client | close every handle, exit | `cap: revoke client ok` twice (original + fork) |
| 10 | controller | close the source; the lifetime case then closes the last handle | object alive while held, stale handle `EBADF`, kernel alive |

## Measured result (2026-11-22)

The old probe passed but only tested handle 3. The strengthened probe was **red** before the fix
on both processes:

```text
cap: never-after errno=1
cap: revoke client exit 1 fork exit 1
cap: failed revoke client exit
init exited (1)
```

`errno=1` is `EPERM`. Handle 4 was created with `READ|DUP`, so `MAP` was never granted and the
contract requires `EACCES`.

After the fix, both the original and the `PROC_FORK` clone print:

```text
cap: revoke eperm ok          (granted MAP, revoked)
cap: dup-before eperm ok      (pre-revoke duplicate that held MAP, revoked)
cap: dup-after ok
cap: dup-after eacces ok      (post-revoke duplicate has READ only)
cap: never-after eacces ok    (READ|DUP handle, MAP never granted)
cap: never-dup eacces ok
cap: revoke client ok
...
cap: ok
init exited (0)
```

## Fix

`kernel/src/process/handle.rs:455 revoke_descendants` called `node.revoke(mask)` on every
descendant. A descendant that never granted a bit recorded it as revoked, so the later lookup in
`crates/raven-handles/src/lib.rs:268-277` saw `revoked & needed != 0` and returned `EPERM`
instead of `EACCES`.

The walk now propagates only the subset the descendant actually granted, before any revoke:

```rust
for node in nodes {
    if !Arc::ptr_eq(&node, target) && node.is_descendant_of(target) {
        let propagated = node.granted().intersect(mask);
        if propagated != Rights::NONE {
            node.revoke(propagated);
        }
    }
}
```

`LineageNode::granted()` is the creation mask. A never-granted bit therefore stays `EACCES`,
while a granted-and-revoked bit stays `EPERM`. `crates/raven-handles/src/lib.rs:273` can
additionally classify with `slot.rights & needed` as defence in depth.

The mutation restores `node.revoke(mask)` and makes R03 red again (`never-after errno=1` on both
processes), proving the probe is not tautological.

## Fixture note

The staged stack is now mapped private (`map::FIXED`, no `SHARED`): a `PROC_FORK` clone must get
COW stack pages. With the shared stack both threads used the same frames and one was killed by a
page fault at `0x2faf0800`.
