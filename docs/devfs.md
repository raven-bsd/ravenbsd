<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Device namespace and `df` statistics (K175)

Status: **OBSERVED at `28e9eae1`.** `/etc/mounts` names `/dev/null`, `/dev/zero`,
`/dev/random` and `/dev/urandom`, but the initfs had no `/dev` directory. `cd /dev` failed with
`ENOENT`, `cat /etc/mounts > /dev/null` failed with `ENOTDIR`, and `df -h` reported the initfs
capacity (`41M 41M 100%`) for every device scheme. The fixed image creates and lists the mount
points, accepts redirection to the devices, and reports zero capacity (`-` usage) for them.

## 1. Profile trace

`profiles/shell.d/40-vfsd.toml` is inherited by the `shell`, `shell-test`, `release`, `ufs-demo`
and other shell-based profiles. It now stages the four mount-point paths:

```toml
[[initfs.files]]
src = "etc/mounts"
path = "/dev/null"

[[initfs.files]]
src = "etc/mounts"
path = "/dev/zero"

[[initfs.files]]
src = "etc/mounts"
path = "/dev/random"

[[initfs.files]]
src = "etc/mounts"
path = "/dev/urandom"
```

The initfs builder creates missing parent directories, so `/dev` exists in every inherited
profile. The placeholder bytes are never visible while the device scheme is mounted over the
path; they only create the directory entry that `ls /dev` lists. No mount is added, removed or
hidden: `/etc/mounts` and `vfsd` keep the same four device rows.

## 2. vfsd fixes

### 2.1 `statfs` follows the opened mount

`userland/servers/vfsd/src/main.rs::Call::Statfs` resolved `/` and proxied the initfs root
regardless of the handle it was called on. Every `fstatfs`/`statfs` therefore returned initfs
statistics, which is where the `41M 41M 100%` device rows came from.

It now proxies the request through the open file's target. A pseudo scheme that does not account
for block or inode usage answers `ENOSYS`; `vfsd` converts that into a zero `StatFs`:

```text
initfs:            85664 85664         0 100% /
ramfs:             16384     4     16380   1% /tmp
null:                  0     0         0    - /dev/null
zero:                  0     0         0    - /dev/zero
rand:                  0     0         0    - /dev/random
rand:                  0     0         0    - /dev/urandom
```

`df -h` omits the zero-capacity pseudo rows (normal `df` behaviour), while `df -a` shows them
with `-` usage. Real mounts still report their own backend numbers; no capacity is fabricated.

### 2.2 `O_CREAT` on an existing mount root

`check_open` tested the parent before the target. A mount root has an empty suffix and its parent
lives outside the mounted scheme, so opening the existing `/dev/null` with `O_CREAT` (shell
redirection) was tested as `null:` being a directory and failed with `ENOTDIR`.

The existence check now runs first. An existing target is checked for access; only `ENOENT`
falls through to the parent check and creation. This matches `open(2)` semantics and leaves
creates inside ordinary directories unchanged.

## 3. Regression

`tests/qtest/cases/dev-namespace.toml` boots `shell-test` under KVM and asserts:

- `cd /dev` followed by `pwd` prints `/dev`;
- `ls /dev` lists four mount points;
- `cat /etc/mounts > /dev/null` exits zero;
- `df -h` exits zero and has no device row carrying the initfs capacity;
- `df -a` shows zero blocks and `-` usage for `null`, `zero` and `rand`;
- no `Not a directory`, `No such file or directory`, or `41M 41M 100% /dev/` appears.

```text
$ timeout 300 cargo xtask qtest --case dev-namespace --arch x86_64
dev-namespace: pass
duration_ms 13422
exit 0
```

The pre-fix run failed with `cd: /dev: no such file or directory` and the corresponding
forbidden line; the log is preserved in the K175 report. The fixed run is `13.4 s`.

## 4. Scope and non-goals

- The root pivot (K176) and the UFS root are unchanged; no kernel change.
- `etc/mounts`, the mount table and `mount` output keep all four device entries.
- No cache or synthetic mount is added; the only new directory entries are the real mount
  points the table already required.

## 5. Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Device mount points | `devfs` supplies a populated `/dev`; no code copied. | FreeBSD `devfs` mount-point outcomes only. | POSIX `open(2)` `O_CREAT`/existing-target semantics; no code copied. | First-party initfs mount points derived from the existing `etc/mounts`. |
| `statfs` routing | VFS `statvfs` is per-mount; outcome only. | `statfs(2)` reports the mounted filesystem; no code copied. | `statfs`/`df` pseudo-filesystem rows; no code copied. | Proxy the open target; zero record for backends that return `ENOSYS`. |
| Zero-capacity reporting | Not applicable. | Not applicable. | GNU `df` prints `-` when block count is zero. | Backend-authoritative numbers; `-` for device schemes. |
