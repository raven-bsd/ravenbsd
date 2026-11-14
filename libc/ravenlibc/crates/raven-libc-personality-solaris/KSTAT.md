<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# kstat: not a libc facility

illumos `libkstat` gives uniform named kernel counters (`06-solaris-design.md` A-sol-60). Raven
makes everything observable through schemes instead:

* `stat:<name>` for kernel counters, one file per counter group;
* `proc:<pid>/{status,ctl,mem,maps,fds,threads/...}` for process state, live and from a core file;
* `raven-libc-db` (ticket K66) over the same files for debuggers.

Therefore C67 provides no `kstat_open`, `kstat_read`, `kstat_chain_update`, `kstat_lookup`,
`kstat_data_lookup` or `include/kstat.h` (`02-api-surface` S05 and R16; `donor-map/14` 3.B.3.e is
SKIP). A program that needs the data opens the `stat:` file, exactly as `ps`-style tools do.
