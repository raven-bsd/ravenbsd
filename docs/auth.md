<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Root password authentication (REL1b)

OBSERVED on `mel/REL1b`. The implementation is the workspace crate `crates/raven-auth`, using
`sha-crypt` **0.6.0** (RustCrypto, `MIT OR Apache-2.0`) with its `password-hash` and `alloc`
features. No hash algorithm is written in this tree; the crate's constant-time verification is
the only comparison.

## Format

The record is one `/etc/shadow` line:

```
name:hash:last_change:min:max:warn:inactive:expire:reserved
```

The hash is **SHA-512-crypt** Modular Crypt Format:

```
$6$[rounds=<n>$]<salt>$<digest>
```

`sha-crypt` defaults to `rounds=5000` and encodes the raw salt argument into the salt field. The
parser accepts the nine fields, reads an empty numeric field as zero (the shadow "unset"
convention), and rejects a wrong field count or a non-integer. Locked hashes (`!...` and `*`) and
empty hashes never verify. `verify_root_shadow` rejects any account other than `root`; there is no
setuid, multi-user or session policy in this crate (REL1 owns that).

## Default root record

`build.rs` generates `DEFAULT_ROOT_HASH` at crate build time from the release password `raven`
and the fixed salt `RavenBSD-root1!`; it writes the constant into `OUT_DIR` and the library
includes it. The image builder installs `default_root_shadow()` into `/etc/shadow`; no plaintext
password file is shipped.

OBSERVED value on this revision:

```
$6$rounds=5000$G3aRZtaEHFI9mxqP$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.
```

The corresponding record is:

```
root:$6$rounds=5000$G3aRZtaEHFI9mxqP$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.:20000:0:99999:7:::
```

`crates/raven-auth/src/hash_ut.rs` checks that the generated hash verifies `raven`, rejects
`raven!`, is deterministic, is a `$6$` record and rejects malformed hashes.

## Independent test vector

The low-level vector is generated with the host `openssl` implementation, not by this crate's
high-level salt encoding:

```
$ openssl passwd -6 -salt r4ven raven
$6$r4ven$S6VNu6h5bJJ4N0puQ63budZes8.I7oATcnLzSQBmkTcTvZ.4QTdkbKttZybkfYqCmRZvvzbfDzxeMs3FalGIQ.
```

`shadow_ut::a_known_vector_verifies` asserts that `verify_password("raven", ...)` accepts it and
`raven!` does not. The same test parses/renderers the shadow record and rejects bad field counts,
bad numbers, locked hashes and non-root accounts.

## Handoff to REL1

REL1's `login` reads `/etc/shadow`, selects the `root` record and calls
`raven_auth::verify_root_shadow(line, password)`; no plaintext comparison and no duplicate hash
logic. The image builder uses `raven_auth::default_root_shadow()` (or `DEFAULT_ROOT_HASH`) to
write the single release record.

## Notes

* SHA-512-crypt is chosen because it is the traditional `/etc/shadow` `$6$` format and the ticket
  explicitly allows it. Argon2 would need a separate policy decision and a parameter/format
  migration; it is not used here.
* The salt of the default release hash is fixed so the build is reproducible. Changing it changes
  the shipped hash; the password remains `raven` for 0.1.0.
* The image-side file installation is REL1's Owns (`etc/shadow`), not this crate's; REL1b provides
  the record and the verifier.
