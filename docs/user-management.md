<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# User management specification (0.2.0)

Status: specification, 2026-11-16. Evidence tags: **OBSERVED** (read in the tree or in a merged
report), **PROPOSED** (this design, not implemented), **UNVERIFIED** (not exercised).

## 1. Scope and non-goals

0.1.0 has one account: `root` with the release password `raven`. 0.2.0 adds a files-backed user
database, non-root login and the administration tools, without changing the 0.1.0 QEMU/Pi 4 login
gate. This document specifies:

* the on-disk account files and their permissions;
* the identity/session model and the kernel boundary;
* the tools `useradd`, `adduser`, `passwd`, `groupadd`, `su`, `id` and `whoami`;
* the libc NSS-files and `raven-auth` boundaries;
* setuid/setgid and filesystem permission requirements;
* the root-only migration and the follow-up tickets U2 (utilities), U3 (kernel identity/setuid)
  and U4 (tests).

Not in 0.2.0: network identity (LDAP/YP), PAM modules, supplementary groups beyond the kernel's
16 entries, ACLs, capabilities/pledge, per-user resource limits, GUI account management, and
`master.passwd` as a second database. Kernel credentials are the identity source; a future job/session manager is policy-only;
0.2.0 keeps the files authoritative so a single-user recovery shell still works.

## 2. Current baseline (OBSERVED)

* `kernel/src/process/mod.rs` already has `ProcessIds` with `uid`, `euid`, `suid`, `gid`, `egid`,
  `sgid`, `groups[16]` and `group_count`.
* `kernel/src/syscall/process_ctl.rs` already serves `PROC_SETCRED_EX` (`0x3B` kinds 1..=9),
  `PROC_CRED_INFO` (`0x3C`) and `PROC_GETGROUPS` (`0x3D`). Effective uid 0 is privileged;
  unprivileged processes may only target their real/effective/saved ids and may not set groups.
* `crates/raven-libc-proc` exposes `getuid/geteuid/getgid/getegid` and the `SetCred` builders to
  the target; host tests use a fake backend.
* REL1b (mel, `c734f06a`) adds `crates/raven-auth`: SHA-512-crypt `$6$` shadow records, a
  constant-time verifier, `verify_root_shadow`, and a deterministic `default_root_shadow()` for
  0.1.0. REL1c/REL1 (still todo) install `/etc/passwd`, `/etc/group`, `/etc/shadow`, `login` and
  the root/`raven` session.
* There is **no** filesystem permission check in `vfsd`/`ramfsd`/`fsd-ufs` today, and no native
  exec path that honours `S_ISUID`/`S_ISGID`. The kernel is still root-only in practice. U3 owns
  that gap.

## 3. Identity model

| Property | 0.2.0 value |
|---|---|
| uid/gid type | `u32`, stored little-endian in the kernel |
| root | uid 0, gid 0, group `wheel` |
| system accounts | uid 1..999, never a login account unless explicitly enabled |
| regular users | uid 1000..59999, allocated monotonically; never reused |
| `nobody` | uid/gid 65534, shell `/nonexistent`, no password |
| primary group | exactly one gid in `/etc/passwd`; same-name group by default |
| supplementary groups | at most 16 (kernel `MAX_GROUPS`); tools reject more |
| sessions | login creates a session (`setsid`) and a process group; kernel P2a fields |
| identity strings | no NUL, no `:`, no newline in names; max name 31 bytes, GECOS 127 |

The **kernel** owns the live credential tuple and its setuid/setgid rules. The **files** are the
source of truth for login and tools. The kernel credential model and tools read/write the same files (via
the same `raven-userdb` library) and may cache lookups; it is not a second database. A corrupted
or missing user database fails closed for non-root login and leaves the root recovery path.

Root is not a normal user for the tools: it may change every account, read `/etc/shadow`, and
`su` to any account. Members of `wheel` may `su` to root. Other users may only change their own
password and may not read the shadow database.

## 4. Account files

### 4.1 `/etc/passwd`

```
name:passwd:uid:gid:gecos:home:shell
```

* Exactly 7 fields. `passwd` is `x` for every account; a non-`x` password field is rejected by the
  parser and never authenticates. `uid`/`gid` are decimal, colon-free.
* `gecos` may be empty and may contain spaces; `home` and `shell` are absolute paths (or empty for
  `nobody`); `shell` defaults to `/bin/brush` for regular users.
* Mode `0644`, owner `root:wheel`. Root record:
  `root:x:0:0:root:/root:/bin/brush`.
* Names are unique and case-sensitive ASCII; uid uniqueness is mandatory; gid must exist in
  `/etc/group` (a missing primary group is a validation error for `useradd`/`login`, not a fatal
  parser error for `getpwnam`).

### 4.2 `/etc/group`

```
name:passwd:gid:members
```

* Exactly 4 fields. `passwd` is `x` or `*`; `gid` is unique decimal; `members` is a comma-separated
  list of names, each present in `/etc/passwd` (validated by tools, not by the raw parser).
* Mode `0644`, owner `root:wheel`. Required records:
  `wheel:x:0:root`, `users:x:100:`, `nobody:x:65534:`.
* `wheel` is the administrative group; `users` is the default group for accounts created without
  `-g`; regular user groups have the same gid as the user's uid when allocated by `useradd`.

### 4.3 `/etc/shadow`

```
name:hash:last_change:min:max:warn:inactive:expire:reserved
```

* Exactly 9 fields, matching REL1b's `ShadowEntry`. Empty numeric fields mean zero; non-integers
  are rejected. `last_change` is days since 1970-01-01 UTC.
* `hash` is SHA-512-crypt MCF `$6$[rounds=<n>$]<salt>$<digest>` for 0.2.0. `!`/`*`/empty means
  locked and never verifies. No plaintext password is ever stored; `passwd` writes a new random
  salt and `rounds=5000` by default.
* Mode `0600`, owner `root:wheel`; it is never installed into the world-readable initfs view in a
  way that a non-root process can open. Only root (or a root-owned auth tool) may read it.
* Root record uses REL1b's deterministic default hash for 0.1.0 compatibility:
  `root:$6$rounds=5000$G3aRZtaEHFI9mxqP$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.:20000:0:99999:7:::`.
* Argon2id is **not** a 0.2.0 format change. It is a later policy decision; a `$argon2id$` parser
  can be added without changing the file shape, but existing `$6$` records must keep verifying.

### 4.4 Homes and skel

* `/home` is `0755 root:wheel`; `/home/<user>` is `0700 <user>:<primary group>`; `/root` is
  `0700 root:wheel`; `/etc/skel` is `0755 root:wheel` and contains `.profile`/`.shrc` defaults.
* `useradd -m` copies `/etc/skel` with the new owner/group; `umask 022` is the session default
  (files 0644, directories 0755).
* The home directory and shell are recorded in `/etc/passwd`; `login` exports `HOME`, `USER`,
  `LOGNAME`, `SHELL`, `PATH=/bin:/usr/bin:/sbin:/usr/sbin` and `TERM` from getty.

### 4.5 Atomic updates

All tools update the three files through one library (`raven-userdb`), never by ad-hoc parsing:

1. take an exclusive lock (`/etc/.pwd.lock`, `O_CREAT|O_EXCL`, bounded wait) or fail `EBUSY`;
2. read and validate all three files;
3. write the new file to a temporary in the same directory, `fsync`, set mode/owner;
4. `rename` into place; update passwd/group/shadow in a fixed order; on a partial failure, restore
   the old content and report `EIO`;
5. release the lock. Plaintext passwords never go through argv, environment or a temp file.

## 5. Tools

| Tool | Owner | Contract |
|---|---|---|
| `useradd` | U2 | `useradd [-m] [-d home] [-s shell] [-u uid] [-g group] [-G list] [-c gecos] name`; allocate 1000+ if `-u` absent; create the same-name group when `-g` is absent; refuse duplicate name/uid/gid; lock the shadow entry until `passwd`; exit 0/1/2 (usage/validation/IO). |
| `adduser` | U2 | Interactive front-end over the same library: name, home, shell, group, password prompt; no separate file format; safe defaults from `useradd`. |
| `passwd` | U2 | `passwd [name]`; root changes any account, a user changes only their own after verifying the old password; writes a fresh `$6$` hash and `last_change`; `-l`/`-u` lock/unlock; no echo; password confirmation; minimum length 8 for non-root; reject NUL/newline. |
| `groupadd` | U2 | `groupadd [-g gid] [-r] name`; allocate 1000+ or a system gid 1..999 with `-r`; refuse duplicate name/gid; no member edit in 0.2.0 (use the library/useradd). |
| `su` | U2 | `su [-] [-c command] [user]`; verifies the target's shadow hash via `raven-auth`; root→any, wheel→root, other users require the target password; sets uid/gid/groups, `HOME`, `SHELL`, `PATH`; `-` starts a login shell; failed auth delay and no password echo; setuid/root-owned as specified by U3. |
| `id` | U2 | `id [-u] [-g] [-G] [-n] [user]`; reads NSS files; prints the effective/real ids and group names; unknown user is exit 1. |
| `whoami` | U2 | Prints the effective user name; unknown uid prints the numeric uid and exits 1. |
| `login`/`getty` | REL1 | Already the root/`raven` 0.1.0 gate; 0.2.0 extends the same binary to look up non-root accounts, home/shell and failed-login handling. Do not fork the auth logic. |

All tools use `raven-userdb` for file access and `raven-auth` for hashes. Option parsing is POSIX
getopt-style with `--`; diagnostics go to stderr; no tool writes a password to the console or a
log. Man pages are required before U2 is accepted.

## 6. NSS files and the auth library boundary

* **`raven-userdb`** (new crate, U2): `no_std` + `alloc`, host-testable, `#![forbid(unsafe_code)]`.
  Parses/renders `passwd`/`group`/`shadow`, validates uniqueness and field ranges, and provides
  lookup/iteration/edit APIs. It never hashes or verifies a password; that is `raven-auth`.
* **libc NSS-files** (U2): `getpwnam`, `getpwuid`, `getpwent`, `setpwent`, `endpwent`,
  `getgrnam`, `getgrgid`, `getgrent`, `setgrent`, `endgrent`, plus the `_r` variants and
  `getgroups`. They read `file:/etc/passwd`/`file:/etc/group` through the POSIX path layer. There
  is no `nsswitch.conf` module loading in 0.2.0; the files backend is the only source. Errors map
  to `ENOENT`/`ERANGE`/`EINVAL`; `struct passwd`/`group` layouts are the frozen ABI.
* **Shadow** is not exposed by ordinary libc. Only a root-owned auth tool or `getspnam` guarded by
  euid 0 may read it; `raven-auth` supplies `verify_password`/`hash_password` and the shadow
  record, and REL1/REL1b's `verify_root_shadow` path stays compatible.
* **Identity boundary** (later): a privileged job/session manager may be the policy writer for session
  and audit policy; it uses `raven-userdb` in-process or through a narrow IPC API. Tools must not
  grow a second parser. The kernel owns credentials; tools link the library; if the identity service is unavailable at
  boot, root recovery works from the files.

## 7. Kernel requirements (U3)

### 7.1 Filesystem permissions

* `Stat` already carries `uid`, `gid`, `mode`; the VFS servers must check the caller's effective
  uid/gid and supplementary groups on `OPEN`, `FSTAT`, directory search and `EXEC`:
  owner/group/other `rwx`, root bypasses read/write checks but still needs an execute bit for
  directories and executable files, and the sticky bit on shared directories (`/tmp`) restricts
  delete/rename to the owner/root.
* `vfsd`/`ramfsd`/`fsd-ufs` receive the caller credential context with every scheme request. The
  current `OPEN` packet has no sender identity; U3 adds a versioned credential field (caller
  uid/euid/gid/egid and bounded group list) to the scheme request or an equivalent
  `PROC`-stamped request. This is a protocol version bump, not a new database.
* `umask` stays a libc/session value in 0.2.0; the client applies it before `OPEN` create mode.
  Kernel-side umask is a later change.

### 7.2 Setuid and setgid execution

* A native ELF with `S_ISUID`/`S_ISGID` and an executable bit runs with effective/saved uid or gid
  set to the file owner/group. Real uid/gid are unchanged; supplementary groups are unchanged;
  root-owned setuid binaries are the only way to gain euid 0.
* The bit is honoured only when the filesystem/mount allows it (a `nosuid` mount disables it) and
  never for scripts or non-ELF images. A setuid file with no execute bit is an error.
* The exec path must obtain the file metadata atomically with the image, not from a path opened
  separately. The expected shape is a native `EXEC`/spawn primitive that takes an open file handle,
  reads its `Stat`, applies the bits, and starts the new process; `PROC_SETCRED_EX` remains the
  credential gate and the existing no-op/privileged rules stay intact.
* `su` and `passwd` are root-owned setuid binaries in the image; they re-check the caller and
  target in userspace as well. If U3's exec path is not ready, 0.2.0 stays root-only and no
  unprivileged `su` is shipped; the spec does not fake a setuid success.
* `setuid`/`setgid` shell scripts are ignored (`ENOEXEC`/no privilege change). The shell must not
  emulate setuid.

### 7.3 Process and session details

* `login` calls `setsid`, sets the controlling terminal, and starts the shell with the account's
  real/effective ids all equal (except a future setuid tool). `su` preserves real uid and sets
  effective/saved ids; `id` reports both.
* `PROC_SETCRED_EX` already enforces the unprivileged allowed-id set; U3 adds tests for the
  groups array, saved-id transitions and the setuid exec path. No new ABI is added unless the
  reviewer approves a contract change.

## 8. Migration from root-only 0.1.0

1. Keep uid 0/root, gid 0/wheel and the REL1b root shadow record byte-for-byte; the 0.1.0
   `root`/`raven` gate must keep working.
2. Add `wheel`, `users`, `nobody` and the empty user database. Do not renumber root.
3. Keep `/etc/passwd` 0644, `/etc/group` 0644, `/etc/shadow` 0600 root:wheel.
4. Existing images have only root; `useradd` is the only way to add accounts. A new image build
   replaces `/etc` as a whole; a running system must use the atomic tools, not a hand edit.
5. If `/etc/passwd` is missing or unreadable, `login` falls back to the root record and failed
   non-root lookups; the recovery shell remains root-only. Never auto-create a user or reset the
   root password on a malformed file.
6. The default `root`/`raven` password is a development credential. 0.2.0 keeps it for the
   release-login gate and warns; the first administrative action should be `passwd root`. A future
   release may force a change, but 0.2.0 must not break REL2b's gate.

## 9. Tests and evidence (U4)

* Unit: parser/renderer round trips, duplicate/invalid fields, uid/gid range, shadow field counts,
  `raven-auth` vectors, NSS lookups from a fixture directory.
* QEMU `user-management`: boot, root login, `useradd -m alice`, `passwd alice`, `id alice`,
  `su - alice`, home ownership/mode, `whoami`, duplicate rejection, locked account, wrong password,
  non-root cannot read `/etc/shadow`, `groupadd`, `id -G`, and root recovery with a malformed file.
* Negative matrix: uid/gid collision, missing group, >16 groups, setuid script, nosuid mount,
  setuid binary without execute bit, malformed shadow, empty password field, path traversal in
  `-d`, and a non-root attempt to edit `/etc/passwd`.
* Fixtures in `tests/fixtures/user-management/`: canonical `passwd`, `group`, `shadow`, expected
  lookup table and test-only hashes. The root release hash is the REL1b vector, not a fixture
  invented here.

## 10. Follow-up tickets

The exact drafts are in `spec/user-management/`:

* **U2 — user utilities and NSS** (`U2-utilities.md`): the library, libc NSS-files, the seven
  tools, man pages and host tests. Depends U1, REL1b, REL1; depends U3 for setuid `su`/`passwd`.
* **U3 — kernel identity, permissions and setuid** (`U3-kernel-identity.md`): credential context in
  scheme requests, VFS permission checks, setuid/setgid exec, nosuid and the negative tests.
  Depends U1, P2a, REL1.
* **U4 — user-management test matrix** (`U4-tests.md`): the QEMU fixture, migration/negative
  tests and the board/QEMU evidence record. Depends U2, U3.

Each draft has exact Owns-globs and acceptance commands. They are drafts for the lead to promote;
this ticket writes no product code.

## 11. Decisions and open questions

* **UM-D1 (PROPOSED):** files are authoritative in 0.2.0; the identity/job manager reads/writes the same files and
  does not own a second database. This keeps the root recovery path independent of a daemon.
* **UM-D2 (PROPOSED):** SHA-512-crypt `$6$` remains the 0.2.0 hash for REL1b compatibility;
  Argon2id is a later format decision, not a reason to break the 0.1.0 hash.
* **UM-D3 (PROPOSED):** the caller credential context is a versioned scheme-request field, not a
  global kernel "current uid" that servers guess. This is the only way `vfsd` can enforce
  permissions for a different process.
* **Open:** whether 0.2.0 ships a setuid `su` or a root-owned identity broker first. The spec
  requires the kernel bit path; the release may keep `su` root-only until U3 lands.
* **Open:** exact `struct passwd`/`group` ABI symbols and NSS `_r` symbols must pass the frozen
  ABI review; U2 lists them but does not edit `crates/raven-abi` without approval.
* **Open:** whether `/etc/master.passwd`/Argon2id from architecture/05 should replace the shadow
  file in 0.3.0; 0.2.0 explicitly keeps `/etc/shadow`.

## 12. Provenance

First-party specification. No donor code was copied. The file formats are the traditional
BSD/POSIX `/etc/passwd`, `/etc/group` and `/etc/shadow` shapes; the hash format and verifier come
from REL1b's `raven-auth` (SHA-512-crypt via the RustCrypto `sha-crypt` crate, MIT OR Apache-2.0).
The identity fields and `PROC_SETCRED_EX` behaviour are from the merged P2a/P2b kernel work.
