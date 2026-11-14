<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->
# Changing behaviour

This file is normative and follows `donor-map/14-headers-abi.md` 3.A.7.

* A bug fix with the same prototype and layout needs no ABI action.
* A change old binaries cannot tolerate needs a new implementation under the same name in a new
  node, bound as the default; keep the old one as an `@RAVEN_1.k` compat symbol.
* Compat symbols live only in `libc.so.1`, never in `libc.a`.
* Removal inside major 1 is forbidden. A rename keeps the old name as an alias and adds the new
  name to the open node.
* A data-symbol size change is a new major.
