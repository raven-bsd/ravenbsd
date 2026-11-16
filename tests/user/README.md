<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# User-tool fixtures and U4 matrix

This directory holds the U2 expected account files and the U4 host matrix crate
(`raven-user-tests`). The matrix drives the seven tool argument layers and `raven-userdb` against
the canonical fixtures and the 0.1.0 image files:

```
cargo nextest run -p raven-user-tests
```

`tests/fixtures/user-management/` remains the canonical input set. No file here is a production
account database, and no fixture contains a plaintext password.

## U2 expected files

| File | Scenario |
|---|---|
| `useradd-carol.passwd` | `useradd -m -c "Carol Example" carol` from the canonical fixture |
| `useradd-carol.group` | the same-name `carol` group with gid 1002 |
| `useradd-carol.shadow` | the locked shadow record created by `useradd` |
| `passwd-alice.shadow` | `passwd alice` replacing the hash and setting `last_change` to 19001 |
| `groupadd-staff.group` | `groupadd staff` allocating the next regular gid |

`check.sh` validates the canonical fixture and checks that the expected files have the
documented shapes. The Rust tests in `crates/raven-userdb/src/db_ut.rs` compare the library output
byte-for-byte with these files.

```
sh tests/user/check.sh
```
