<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# User-management test matrix (U4)

`tests/user` is the host matrix for the U2 tools. It drives the tool argument layers and the
`raven-userdb` operations together against the canonical fixtures and the real 0.1.0 image
files. It does not modify the U2 production code.

```
cargo nextest run -p raven-userdb
cargo nextest run -p useradd -p adduser -p passwd -p groupadd -p su -p id -p whoami
cargo nextest run -p raven-user-tests
make check-fast
```

The target QEMU case is deferred until U3b provides setuid and non-root credentials. A host test
cannot honestly exercise the target's `su`/`passwd` credential switch before that.

## Matrix

| Case | Expected | Observed | Result |
|---|---|---|---|
| `useradd -c "Carol Example" carol` | passwd/group/shadow rows uid/gid 1002, locked shadow | matches `useradd-carol.*` byte-for-byte | pass |
| `adduser dave` defaults | `/home/dave`, `/bin/brush`, same-name group, hashed shadow | uid 1002, hash verifies, no plaintext in files | pass |
| duplicate user name | `DbError::Duplicate` | duplicate rejected | pass |
| duplicate explicit uid | `DbError::Duplicate` | duplicate rejected | pass |
| duplicate group name/gid | `DbError::Duplicate` | duplicate rejected | pass |
| `passwd alice` | fresh `$6$` hash, `last_change=19001` | hash verifies; fixed U2 vector matches `passwd-alice.shadow` | pass |
| `passwd -l/-u alice` | `!` prefix then restore | lock/unlock round trip | pass |
| `groupadd staff` | next regular gid 1002 | matches `groupadd-staff.group` | pass |
| `groupadd -r svc` | next system gid 101 | gid 101 | pass |
| `groupadd -g 2000 ops` | explicit gid | gid 2000 | pass |
| `su -c id root` | root environment, `-c` arguments | HOME/SHELL/USER/LOGNAME/PATH/PWD correct | pass |
| `su - alice` | login environment | `PWD=/home/alice`; parsing stable | pass |
| `id` root | `uid=0(root) gid=0(wheel) groups=0(wheel)` | exact | pass |
| `id alice` | uid/gid/group 1000 | exact | pass |
| `id -G` after adding `users` | `1000,100`; `-Gn` `alice,users` | exact | pass |
| `id` unknown user | exit 1 / `DbError::UnknownUser` | rejected | pass |
| `whoami` | root, alice, numeric unknown | exact | pass |
| canonical fixture structure | unique names/ids, valid refs, `$6$` or lock | 4 users, 5 groups, 3 shadow rows | pass |
| 0.1.0 root account | parse/render byte-for-byte; REL1b hash verifies `raven` | exact | pass |
| 0.1.0 + `useradd alice` | uid/gid 1000, root unchanged | root passwd/shadow unchanged | pass |
| 0.1.0 + `useradd -g users bob` | needs the U1 group migration | `UnknownGroup` | finding |

## Fixtures

`tests/fixtures/user-management/{passwd,group,shadow,expected.tsv,check.sh}` are the canonical
0.2.0 inputs. `tests/user/{useradd-carol.*,passwd-alice.shadow,groupadd-staff.group}` are the U2
post-operation expectations. No fixture contains a plaintext password; the shadow rows are
`$6$` hashes or lock markers.

## Findings

1. **0.1.0 image group migration is incomplete.** `etc/group` contains only
   `root:x:0:root`; `wheel`, `users` and `nobody` are absent. `useradd alice` works without `-g`
   (it creates the same-name group), but `useradd -g users bob` returns `UnknownGroup` on the
   current image. U1 section 8 requires the 0.2.0 image to add those three groups; this is an
   image/profile task, not a U2 tool defect. The canonical fixture already has them.
2. **Root shell differs between the image and U1.** The 0.1.0 `etc/passwd` root shell is
   `/bin/rsh` (release commit `996bd795`), while the U1/canonical 0.2.0 model uses `/bin/brush`.
   The matrix pins the actual 0.1.0 shell for the compatibility case and the brush shell in the
   canonical fixture; the 0.2.0 image migration must choose the final shell.
3. **Non-root/setuid paths are not exercised.** `su` implements only root-to-root in 0.2.0 and
   reports the U3 gap for other accounts; `passwd` has no setuid path; `id`/`whoami` report root.
   These are documented U2 handoffs and become U3b/QEMU cases once credential switching exists.
4. **Password salt encoding.** `raven_auth::hash_password` encodes raw salt bytes into the `$6$`
   salt field, so a raw `b"testsalt"` does not reproduce the OpenSSL literal-salt vector used by
   `passwd-alice.shadow`. The matrix tests both a freshly generated/verified hash and the fixed U2
   vector; this is expected, not a password bug.

## Evidence

```
cargo nextest run -p raven-user-tests
Summary [0.570s] 13 tests run: 13 passed, 0 skipped

make check-fast
check-fast: PASSED
```
