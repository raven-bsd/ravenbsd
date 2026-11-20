<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# pfctl / pf.conf fixtures

Input fixtures for the K274 `pfctl` parser and loader. They are plain pf.conf text; the
expected verdict for each file is in `manifest.tsv`, and the self-test only checks the file
set and manifest shape until K274 provides `raven-pfctl`.

The accepted subset is the K274 contract: rules (pass/block, quick, in/out, on, proto,
from/to, addresses/CIDR/ports, `keep state`), tables, NAT/redirect/binat, and the
`set skip`/`set optimization`/`set timeout`/`set limit`/`scrub` options. Malformed inputs
must be rejected as a whole; no partial ruleset is loaded.

Donor references: NetBSD `dist/pf/sbin/pfctl/parse.y`, `pfctl_parser.c`, `pfctl_table.c`,
`pfctl.c` and `dist/pf/share/man/man5/pf.conf.5` (TABLES, OPTIONS, TRAFFIC NORMALIZATION,
TRANSLATION); FreeBSD `sys/netpfil/pf/pf.c`/`pf_table.c` for the runtime semantics. No donor
code is copied.

`manifest.tsv` columns: `file|status|line|construct|note`. For `status=error`, `line` is the
1-based line the parser must name. Diagnostic text is deliberately not pinned because K274 owns
the exact wording.
