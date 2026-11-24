# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# OpenSSH portability patches

No upstream OpenSSH file is patched yet. The build is blocked before `configure` by the T1
OpenSSL dependency, so no portability change has been justified or tested.

Rules for any patch added here:

* One patch per upstream file and reason; never touch SSH protocol, cipher suites, KEX, key
  formats or crypto.
* Prefer a configure feature selection or a sysroot/build workaround before a source patch.
* Keep the vendored `addons/ssh/openssh/` tree unmodified; the only exception so far is the
  exclusion of the upstream `regress/` test tree (including its private-key fixtures) for GitHub
  push protection, recorded in `PROVENANCE.toml` and `.provenance-ignore`.
