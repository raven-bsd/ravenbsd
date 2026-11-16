<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# User management tools (U2)

Status: implemented on `mel/U2`. This page documents the 0.2.0 files-backed user database and the
seven administration utilities. The normative model is `docs/user-management.md`; this page is the
implementation and handoff record.

## Scope

U2 implements:

* `crates/raven-userdb`: `no_std` + `alloc`, `#![forbid(unsafe_code)]`, host-testable parsing,
  rendering, validation, lookup and locked temporary-file edits for `/etc/passwd`, `/etc/group`
  and `/etc/shadow`;
* `useradd`, `adduser`, `passwd`, `groupadd`, `su`, `id` and `whoami` under `userland/bin/`;
* the U2 fixtures under `tests/user/` and man pages under `docs/man1/`.

It does **not** implement the libc NSS-files surface from the draft
`spec/user-management/U2-utilities.md`: the promoted ticket's Owns list does not include
`libc/ravenlibc/**`, and the frozen `struct passwd`/`group` ABI needs a separate review. The
library and tools are ready for that adapter.

It does not implement setuid/setgid execution, kernel permission checks or real credential
retrieval. Those are U3.

## Tool matrix

| Tool | Invocation | Account effect | Password effect | Exit codes |
|---|---|---|---|---|
| `useradd` | `useradd [-m] [-d home] [-s shell] [-u uid] [-g group] [-G list] [-c gecos] name` | adds passwd; creates/reuses same-name primary group; adds supplementary memberships | locked `!` shadow record | 0 ok, 1 validation/I/O, 2 usage |
| `adduser` | `adduser [name]` | same as `useradd`, prompts for every field | prompts twice, hashes, writes the record | 0 ok, 1 validation/I/O, 2 usage |
| `passwd` | `passwd [-l\|-u] [name]` | none | `$6$` hash, lock/unlock | 0 ok, 1 validation/I/O, 2 usage |
| `groupadd` | `groupadd [-g gid] [-r] name` | adds empty group | none | 0 ok, 1 validation/I/O, 2 usage |
| `su` | `su [-] [-c command] [user]` | none | verifies target hash when a non-root caller is possible | 0 ok, 1 failure, 2 usage |
| `id` | `id [-u] [-g] [-G] [-n] [user]` | none | none | 0 ok, 1 unknown user, 2 usage |
| `whoami` | `whoami` | none | none | 0 known uid, 1 unknown uid, 2 usage |

All tools are root-only in 0.2.0. `passwd` does not accept a password on the command line, in the
environment or in a temporary file. `useradd` and `adduser` create the shadow record locked; the
first `passwd` call installs a hash.

## `raven-userdb`

The crate is split into small modules:

| Module | Responsibility |
|---|---|
| `passwd` | seven-field records, name/gecos/path validation, render |
| `group` | four-field records, member list add/remove, render |
| `shadow` | nine-field records, lock/unlock, numeric validation |
| `db` | parse all files, cross-file validation, lookups, id allocation, `add_user`/`add_group`/`set_password` |
| `store` | `DbIo` seam, `Paths`, `Transaction`, lock/temp/rename/rollback |
| `rt` | target-only `raven-rt` adapter, console prompts, `/etc/skel` copy, random bytes |

The parser rejects non-`x` passwd markers, non-decimal ids, non-absolute or `..` paths, malformed
gecos, duplicate names/ids, unknown primary gids, unknown group members and orphan shadow records.
A shadow record is not required for every account (`nobody` has none in the canonical fixture).

The hash format is checked as `$6$...` or a lock marker, but `raven-userdb` never hashes or
verifies. `raven-auth` remains the only SHA-512-crypt implementation.

## Atomic update

`Transaction::begin(RtIo, Paths::system())`:

1. creates `/etc/.pwd.lock` with `O_CREAT|O_EXCL`, retrying a bounded number of times;
2. reads all three files through `DbIo` and parses/validates the whole database;
3. returns the mutable `Database` to the tool.

`commit()`:

1. validates the database again;
2. creates `<path>.tmp` in the same directory, applies `0644`/`0644`/`0600` **before** writing
   any bytes (so a shadow temporary never holds a hash at the server's default mode), flushes;
3. renames passwd, group and shadow in that fixed order;
4. on a rename failure, restores the files already replaced from the bytes read in `begin` and
   returns `EIO` if the restore itself fails;
5. removes the lock. `abort()` and `Drop` also remove it.

`id` and `whoami` use `load_lookup()`, which reads only passwd and group. `su` uses `load()`.

## Fixtures and tests

The canonical parser vectors remain in `tests/fixtures/user-management/`. U2 adds expected
post-operation files in `tests/user/`:

| File | Scenario |
|---|---|
| `useradd-carol.passwd`, `.group`, `.shadow` | `useradd -c "Carol Example" carol` |
| `passwd-alice.shadow` | `passwd alice` with a fixed test hash and `last_change=19001` |
| `groupadd-staff.group` | `groupadd staff` |

`crates/raven-userdb/src/*_ut.rs` compares the library output byte-for-byte with those files and
exercises lock contention, rollback, UTF-8 rejection, duplicate ids, range exhaustion and the
root-allocation rules. `tests/user/check.sh` runs the canonical structural check plus the U2
fixture shapes.

## U3 and image handoffs

The following are deliberately not faked in U2:

* **Real credentials.** `raven-rt` has no safe wrapper for `PROC_SELF_INFO`/`PROC_CRED_INFO`, and
  the kernel is root-only. `id`/`whoami` therefore report root, and `su` implements only the
  root-to-root path. `su alice` reports the U3 gap and exits 1.
* **Setuid and non-root `passwd`.** `passwd` has the root path only. The own-password verification
  path needs a setuid binary and kernel credentials.
* **Echo suppression.** `read_line` reads canonical `tty:console` input. The console server echoes
  the typed bytes; disabling echo needs terminal-mode support, which is not in U2.
* **Home ownership.** The VFS has no `chown` in this slice. `-m` creates the home with mode 0700 and
  copies `/etc/skel`, but the files remain owned by the caller (root). The mode is correct.
* **Writable `/etc`.** The 0.1.0 image serves `/etc` from the read-only initfs. Atomic updates
  require a writable root filesystem, or a writable `/etc` mount, before the tools can change a
  running image. Profile/rootfs wiring is outside U2.
* **NSS.** No libc symbols were added; see the scope note above.
* **Target build of `raven-auth`.** The pinned `sha2` 0.11.1 currently fails LLVM codegen for
  `x86_64-unknown-none` (`rustc-LLVM ERROR: Do not know how to split the result of this
  operator!`). Forcing the portable SHA-2 backend with
  `RUSTFLAGS='--cfg sha2_backend="soft"'` builds `raven-auth`, `passwd` and `adduser`; the
  `make check-fast --fast` gate skips target clippy, so it is not visible there. This is
  pre-existing REL1b work and affects the login binary too. The wave `make check` or image build
  needs that cfg in the build configuration (or a sha2 pin).

## Evidence

```
cargo nextest run -p raven-userdb          # 57 tests
cargo nextest run -p useradd               # 7 argument tests
cargo nextest run -p passwd                # 7 argument and password-policy tests
sh tests/user/check.sh                     # canonical + U2 fixture shapes
make check-fast                            # PASSED
```

The seven man pages are in `docs/man1/`. The repository docs gate requires one page per
`userland/bin` program, so they were added even though the promoted U2 Owns list named only
`docs/user-tools.md`.
