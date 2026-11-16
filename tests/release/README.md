<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release verification tests

`scripts/release-verify.sh` is the independent clean-clone release verifier. This directory holds
its non-build host checks:

```sh
sh tests/release/check-script.sh
```

`check-script.sh` checks the shell syntax, the `--help` path, the `/tmp` scratch-directory guard
and the extra-argument guard. It does not clone, build or run QEMU; the full verification is:

```sh
sh scripts/release-verify.sh /tmp/opencode/release-verify
```

See `docs/release-verify.md` for the verifier flow and evidence.
