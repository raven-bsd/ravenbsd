<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Caller credentials and filesystem permissions (U3a)

OBSERVED on `stuart/U3a` (base `origin/master` `dd0b95a3`). Host tests, kernel checks and two
qtests; no board claim.

## Scheme packet v2

The S11 scheme packet is now **version 2** and `PACKET_SIZE` is **520** bytes (was 440). The
existing fields keep their offsets; the credential block is appended after `attach`:

| Offset | Field | Meaning |
|---|---|---|
| 436 | `uid` | real user id |
| 440 | `euid` | effective user id |
| 444 | `gid` | real group id |
| 448 | `egid` | effective group id |
| 452 | `group_count` | valid supplementary groups, bounded by `MAX_GROUPS` (16) |
| 456 | `groups[16]` | supplementary group ids |

`kernel/src/ipc/mod.rs` fills the block in `PendingRequest::deliverable_packet` from
`request.client.ids`. `raven-scheme::Ctx` exposes it as
`raven_proto_fs::Credentials` to `open_with` and the new `*_with` handler methods. Old version-1
packets are rejected explicitly; all in-tree peers are rebuilt together.

## Permission model

`raven_proto_fs::Credentials` implements the class selection and checks:

* owner when `euid == owner`; otherwise group when `egid` or a supplementary group matches;
  otherwise other;
* each class is checked against the mode bits for read (`4`), write (`2`) and execute (`1`);
* root (effective uid 0) bypasses read/write but still needs at least one execute bit for `X`;
* a sticky directory (`0o1000`) limits unlink/rename to the file owner or root.

`Call::Access { mode, path }` (opcode 13) checks `R`/`W`/`X` at a path for the caller. It is the
seam U3b uses for the exec bit; a 0644 file fails `X` for root and non-root.

## Enforcement

* **`vfsd`** resolves `file:` paths and preflights caller access against the target server's
  `Stat` for `OPEN` (including an existing target with `O_CREAT`/`O_TRUNC`), parent
  write/execute for create, every traversed directory's execute bit, `CALL` mkdir/unlink/rmdir/
  rename/symlink/readlink/chmod/truncate, sticky deletion, and `Access`. It then forwards the
  operation; the target server applies its own check for direct scheme clients.
* **`ramfsd` / `raven-ramfs-core`** store `uid`/`gid` per inode (root starts at 0/0). Creation
  records the caller's effective ids, traversal checks directory execute, open checks
  read/write, path operations check parent write/execute, `chmod` requires owner/root and sticky
  deletion is owner/root only.
* **`fsd-ufs`** checks every traversed directory execute bit and the final inode read/write/
  execute in `open`, plus `Readlink` and `Access`. Its create/unlink/rename operations remain
  `EROFS` as before (read-only UFS slice).
* The 0.1.0 root login path stays green: root uid 0 reads `/etc` and the login session passes.

The kernel owns the live tuple; `process::ProcessIds` remains the authority. The client never
sends a credential field itself, so it cannot forge its uid/gid.

## Fixture matrix (OBSERVED)

`tests/qtest/cases/perm.toml` builds a small initfs with the fixture plus `initfsd`, `ramfsd` and
`vfsd`, starts them in a namespace-safe order, then prints:

```
perm: services ok
perm: root read=ok exec=denied
perm: uid1000 0600=denied 0644=ok
perm: uid1000 dirs 0755=ok 0700=denied
perm: uid1000 exec 0644=denied
perm: uid1000 sticky=denied
perm: ramfs 0600=denied 0644=ok dir700=denied
perm: ok
init exited (0)
```

The fixture creates a 0600 root file, a 0644 file, 0755/0700 directories, a 0644 executable and a
sticky `/tmp/sticky/rootfile`, checks the root bypass, drops to uid/gid 1000 with
`PROC_SETCRED_EX`, and checks `file:` (vfsd) and direct `ramfs:` behavior.

## Evidence

```
$ cargo nextest run -p raven-ipc -p raven-proto-fs -p raven-vfs-core -p raven-ramfs-core \
    -p raven-scheme -p raven-ramfsd -p fsd-ufs -p raven-blockd
Summary: 339 tests run: 339 passed

$ cargo check -p raven-kernel --target x86_64-unknown-none
exit 0
$ cargo check -p raven-kernel --target aarch64-unknown-none-softfloat
exit 0
$ cargo xtask qtest --case perm --arch x86_64
perm: pass
$ cargo xtask qtest --case tests/login/session/login-session.toml --arch x86_64
login-session: pass
$ make check-fast
check-fast: PASSED (11546 host tests, 4 skipped)
```

## Limits

* `vfsd` performs the target open/call as the server process after preflighting the caller; files
  created through the `file:` proxy are owned by the target server (root) in this slice. Direct
  `ramfs:` creation records the caller's uid/gid. A follow-up should pass an explicit create
  owner through the proxy or impersonate the caller around the target open.
* `fsd-ufs` is read-only; create/unlink/rename stay `EROFS`.
* Actual setuid/setgid exec and `nosuid` are U3b; this ticket provides the `Access` check and the
  credential context they need.
* `initfsd` has no permission checks of its own (not in U3a's Owns); `file:` access to the initfs
  is checked by `vfsd`. Direct `initfs:` scheme use is root/infrastructure-only today.
