<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# shim

Replacement headers for donor files that include a libc-private header the tree does not build
(`namespace.h`, `local.h`, `libc_private.h`, ...). The directory starts empty: the ticket whose donor
file needs a header writes a minimal replacement **from scratch** here (guide 6.2), never copies it
from another project, and lists it in the report.

A recipe uses one with `include = ["shim:<dir>"]`; the tool resolves it to this directory and puts
it on the compiler's `-I` path. Donor header directories use `include = ["donor:<donor>/<dir>"]`.
