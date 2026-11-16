<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# U2 user-tool fixtures

These are the expected account files after the U2 library operations exercised by
`cargo nextest run -p raven-userdb`. They extend the canonical vectors in
`tests/fixtures/user-management/`; no file here is a production account database.

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
