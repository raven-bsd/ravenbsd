<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# User-tools image overlay (U5)

The U5 overlay stages the seven U2 tools without changing the 0.1.0 `release` default:

```
profiles/user-tools.toml
profiles/user-tools.d/10-services.toml
profiles/user-tools.d/20-tools.toml
profiles/user-tools.d/30-rc.toml
profiles/user-tools.d/rc.conf
```

`user-tools` extends `base` (not `shell`) so the first-party `id` is not shadowed by the uutils
`/bin/id`. It adds the log/initfs/ramfs/vfs/console/rsh services, `/etc/passwd`, `/etc/group`,
`/etc/shadow`, the seven binaries, their man pages, and the `useradd`/`adduser`/`passwd`/
`groupadd`/`su`/`id`/`whoami` paths:

| Repository source | Image path |
|---|---|
| `userland/bin/useradd` | `/bin/useradd` |
| `userland/bin/adduser` | `/bin/adduser` |
| `userland/bin/passwd` | `/bin/passwd` (4755 in the qtest image) |
| `userland/bin/groupadd` | `/bin/groupadd` |
| `userland/bin/su` | `/bin/su` (4755 in the qtest image) |
| `userland/bin/id` | `/bin/id` |
| `userland/bin/whoami` | `/bin/whoami` |
| `docs/man1/*.1` | `/usr/share/man/man1/*.1` |

The image build does not modify `profiles/release.toml`, `profiles/release.d/**`, the REL1c
`etc/passwd`/`etc/group`/`etc/shadow` templates, or any U2 tool source.

## qtest image

The stock initfs serves `/etc` read-only. The qtest fixture under `tests/user/image/` builds a
custom initfs with:

* all seven tools, with `passwd` and `su` packed mode `4755`;
* a `user-image-init` PID 1 that starts logd/initfsd/ramfsd/nulld/zerod/randd/vfsd/consd,
  mounts a ramfs over `/etc`, copies `passwd`/`group`/`shadow` from the initfs, and starts
  `/bin/rsh`;
* the U2 tools then update the writable account database.

```
cargo xtask qtest --case user-tools --arch x86_64
```

The fixture is test-only. A production user-tools image needs a writable rootfs (or the same
ramfs bootstrap) and the setuid modes; the standard profile stages the binaries and services, while
the qtest proves the create/passwd/su flow. The 0.1.0 release image and its root login profile are
unchanged; `cargo xtask qtest --case release-login --arch x86_64` remains the root-login gate.

The qtest uses `useradd -s /bin/rsh alice` so the fixture does not need the brush image; the
normal account default remains `/bin/brush` in `raven-userdb`.

## Transcript

```
$ cargo xtask qtest --case user-tools --arch x86_64
user-tools: pass

build/qtest/user-tools.json:
  "result": "pass",
  "matched": [
    "^rsh: ready$",
    "^New password: ?$",
    "^Retype new password: ?$",
    "^uid=1000\\(alice\\) gid=1000\\(alice\\) groups=1000\\(alice\\)$",
    "^root$",
    "^alice$",
    "^uid=1000\\(alice\\) gid=1000\\(alice\\) groups=1000\\(alice\\)$",
    "^root$",
    "^init exited \\(0\\)$"
  ],
  "unmatched": [],
  "forbidden_hits": []
```

The serial log records `useradd`, the two `passwd` prompts, `id alice`, root `whoami`, `su alice`,
the nested `whoami`/`id` as alice, and the return to the root prompt. The password is not echoed
and no plaintext hash appears in the image.

The exact serial transcript is recorded in `tickets/mel/reports/U5.md`.

## Build note

The nested `passwd`/`su` binaries depend on `raven-auth`; the target build carries
`--cfg sha2_backend="soft"` because the pinned `sha2` 0.11.1 fails LLVM codegen on
`x86_64-unknown-none`. `tests/user/image/build.sh` sets that flag for the fixture.
