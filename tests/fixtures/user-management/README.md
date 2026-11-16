<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# User-management fixtures

These files are parser and lookup vectors for U2/U4, not a production account database. The
canonical set is:

| File | Purpose |
|---|---|
| `passwd` | root, two regular users and `nobody` with unique uids |
| `group` | `wheel`, `users`, same-name groups and `nobody` |
| `shadow` | REL1b's release root vector, a test `$6$` vector for `alice` and a locked `bob` |
| `expected.tsv` | lookup expectations for the parser/NSS tests |
| `check.sh` | structural validation; it does not authenticate anyone |

The root record is REL1b's deterministic release hash for the 0.1.0 password `raven`; keep it in
sync with `crates/raven-auth`. The `alice` hash is the independent OpenSSL vector from
`docs/auth.md` (also `raven`); it is test-only. No plaintext password is stored here.

Run the structural check with:

```
sh tests/fixtures/user-management/check.sh
```
