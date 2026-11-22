<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Users

OBSERVED (host tests and the `raven-users`/`raven-ufs` qtests): `raven config` and
`raven users` manage the files-backed account database `/etc/passwd`, `/etc/shadow` and
`/etc/group` plus the console autologin drop-in `/etc/raven/autologin.toml`. The model is
`crates/raven-config/src/users.rs`; the TUI forms are in `crates/raven-tui`; the target CLI
records are in `userland/bin/raven/src/admin.rs`.

The admin client is installed at `/usr/sbin/raven`; `/usr/sbin` is in the root PATH, so
`raven ...` and `/usr/sbin/raven ...` are equivalent.

## Base accounts

The release and UFS images stage two accounts from the same source files. The 0.1.0 password
for both is `raven`.

```text
root:x:0:0:root:/root:/bin/rsh
raven:x:1000:1000:raven:/home/raven:/bin/rsh
```

| Account | uid/gid | Home | Shell |
|---|---|---|---|
| `root` | 0/0 | `/root` | `/bin/rsh` (login policy runs `/bin/brush`) |
| `raven` | 1000/1000 | `/home/raven` | `/bin/rsh` |

`luser` is not a base account. K195's SSH profile uses `raven`; it must not duplicate the
account. The release profile stages `/home/raven`; the `raven-users` fixture creates the same
directory in its private UFS disk because `qemu-ufs` extends `shell`, not `release`. A runtime
save creates and chowns new homes (the admin runs as root). A non-root preview root keeps the
caller's ownership because the kernel denies `chown`.

The `raven` shadow record is a standard `$6$` vector generated independently with
`openssl passwd -6 -salt K229ravenBase01 raven`; runtime changes and new accounts use the
first-party `raven-auth` SHA-512-crypt path.

## Fields and validation

| Field | Rule |
|---|---|
| Username | 1-31 bytes, starts with a letter or `_`, then letters/digits/`.`/`_`/`-`; unique |
| UID/GID | Regular ids 1000-59999; unique uid; root keeps 0/0 |
| Home | absolute; empty means `/home/<name>`; created on save |
| Shell | absolute, executable file; a TUI app is a valid shell |
| Password | required for a new account; 1-128 bytes, no control characters; never stored plaintext |
| Autologin | at most one console account |

Root cannot be deleted or have its uid/gid/home changed; its password may be changed. A save
validates the whole edited set before writing.

## Passwords

A password is hashed with `raven-auth::hash_password` (SHA-512-crypt, `$6$`) and 16 random
characters from `/dev/urandom` mapped to the crypt alphabet. `/etc/shadow` is written atomically
with mode 0600; the other two files use 0644. A password change replaces only that account's
shadow hash. `--dry-run` prints `password = "<set>"` or `"<unchanged>"`, never the secret.

The runtime hash path requires `/dev/urandom`; if it is unavailable the save fails instead of
falling back to a predictable salt.

## Shells and the musicbox pattern

A user's passwd shell is the program login/getty start after authentication. To make a TUI or
kiosk app a shell, create the account with an absolute executable shell:

```text
musicbox  uid/gid 1000  /home/musicbox  /bin/tui-demo
```

`/bin/tui-demo` is an ordinary executable (the qtest uses a static stub that prints
`tui-demo: ready`). A real jukebox would set the same field to its TUI binary. SSH runs the
passwd shell in a PTY after its own authentication; this ticket does not change OpenSSH or PAM.
The `ssh musicbox@` end-to-end proof is K195 acceptance, not claimed here.

## Console autologin

`/etc/raven/autologin.toml`:

```toml
# Managed by raven config (K229).
[console]
user = "musicbox"
enabled = true
```

`getty` reads the file at start. With `enabled = true` and an existing account it starts that
account's passwd shell without a password prompt; an unknown/invalid user falls back to the
normal `login:` prompt. SSH never reads this file. The default image installs the file disabled.
The TUI exposes the switch on each account form; enabling one account disables the previous one.

## TUI

`raven config` lists **Users (add)** and one **Users: <name>** entry per account. The Users
forms are searchable like every module:

* the add form sets username, uid/gid, home, shell, password/confirm and autologin; uid/gid
  default to the next free regular ids and the home to `/home/<name>`;
* an existing form changes uid/gid/home/shell, keeps the current hash when both password fields
  are empty, and has a `Delete` choice;
* root's uid/gid/home/shell/delete fields are read-only; only the password can change;
* Save validates and writes; `--dry-run` prints the plan and names `/etc/passwd` as the target
  with `file=present`, writing nothing.

## Scriptable surface

```sh
raven users list
raven users show musicbox
raven users check
raven --json users list
raven --json users show musicbox
raven --json users check
```

`list`/`show` emit one `user` record (`name`, `uid`, `gid`, `home`, `shell`, `autologin`,
`password_set`); the hash is never emitted. `check` emits a `check` record with `status`, user
and group counts and an `errors` array, and exits 1 when the database is inconsistent (file
modes, duplicate ids, missing groups/shadow records, non-executable shells, missing homes,
invalid autologin).

## Tests

* `cargo nextest run -p raven-config -p raven-tui -p raven -p login -p raven-getty` covers the
  model, hashing, atomic apply, TUI save/dry-run, CLI records and the getty/autologin policy.
* `tests/qtest/cases/raven-users.toml` boots the one-disk UFS demo, creates `musicbox` with the
  stub `/bin/tui-demo` through the TUI, reads the three files, verifies the `$6$` hash, the home,
  the autologin file, authenticates with `/bin/login`, runs the passwd shell, and checks the JSON
  records.
* `tests/qtest/cases/raven-ufs.toml` remains the writable-UFS config/TUI regression.

## Donor reconciliation

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| user/password management | behaviour only; CDDL, no copy | FreeBSD/NetBSD passwd/shadow outcomes only | Linux useradd/chpasswd outcome only | first-party TUI/config; `raven-auth` SHA-512-crypt; no donor code |
| autologin | SMF/getty outcome only | BSD getty autologin outcome only | systemd/getty outcome only | first-party getty/login config; no donor code |

No donor code is copied.
