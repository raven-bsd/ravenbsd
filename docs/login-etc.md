<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Login account files and service fragment (REL1c)

OBSERVED on `mel/REL1c`. REL1b supplies the SHA-512-crypt verifier and the build-time root hash;
this ticket adds the image-side account files and the getty/login profile fragment. REL1 owns the
`getty`/`login` binaries and the session logic.

## Files

| Path in the image | Repository source | Image mode | Purpose |
|---|---|---|---|
| `/etc/passwd` | `etc/passwd` | `0644` | one root account, shell `/bin/brush` |
| `/etc/group` | `etc/group` | `0644` | root group (gid 0) |
| `/etc/shadow` | `etc/shadow` | `0600` | root SHA-512-crypt `$6$` hash |
| `/etc/profile` | `etc/profile` | `0644` | PATH, HOME, TERM, USER, LOGNAME, SHELL |
| `/etc/motd` | `etc/motd` | `0644` | release welcome text |
| `/etc/services.d/getty.toml` | `profiles/release.d/login.service.manifest` | `0644` | one getty on `tty:console` |

`initfsd` installs the initfs entries as the root identity; the initfs format stores only the
mode, not a uid/gid. The `0600` shadow mode and `0644` account/profile modes are set in the
profile fragment and in the repository working tree. The repository owner is the checkout owner,
not root.

## Root record

`etc/shadow` is exactly the REL1b build-time default record:

```
root:$6$rounds=5000$G3aRZtaEHFI9mxqP$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.:20000:0:99999:7:::
```

No plaintext password is present. `tests/login/etc/check-etc.sh` checks the modes, the `root`
account/group rows, the `$6$` prefix and the absence of a plaintext `root:raven` record.

## Service fragment

`profiles/release.d/login.toml` installs the future `raven-getty` and `raven-login` packages at
`/bin/getty` and `/bin/login`, the five account files, and the service manifest. The manifest is
`profiles/release.d/login.service.manifest`:

```toml
name = "getty"
exec = "/bin/getty"
needs = ["consd", "vfsd", "ramfsd", "initfsd"]
restart = "on-failure"
grants = ["initfs"]
```

The fragment lives under `profiles/release.d/` so it is consumed when REL2a adds the release
profile; it does not change the current `shell` profile, whose `rsh` service predates getty.
REL1 adds the two binary crates and owns the getty/login session itself.

## Evidence

```
$ sh tests/login/etc/check-etc.sh
REL1c: etc/passwd mode 644 ok
REL1c: etc/group mode 644 ok
REL1c: etc/shadow mode 600 ok
REL1c: etc/profile mode 644 ok
REL1c: etc/motd mode 644 ok
REL1c: shadow hash ok
REL1c: ok
```

```
$ stat -c '%a %n' etc/passwd etc/group etc/shadow etc/profile etc/motd \
    profiles/release.d/login.toml profiles/release.d/login.service.manifest
644 etc/passwd
644 etc/group
600 etc/shadow
644 etc/profile
644 etc/motd
644 profiles/release.d/login.toml
644 profiles/release.d/login.service.manifest
```

The `raven-auth` unit tests (including the shadow parser and the generated default hash) pass on
this revision.
