#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1f8 primitive/timer row matrix. This is a report artifact for the P1f9 row audit; it does not
# change the gate. Each line names the row class, the implementation state and the exact owner or
# evidence path. A row is never silently waived.

set -eu

printf '%-24s %-10s %-16s %s\n' ROW CLASS STATE OWNER/EVIDENCE
printf '%-24s %-10s %-16s %s\n' 'interface.alarm' required implemented \
    'raven-libc/src/unistd/alarm.rs: libc timer table + detached TIMER_CREATE thread delivering SIGALRM via PROC_SIGNAL (kernel/src/syscall/process_ctl.rs:0x4A); Symbol.map RAVEN_1.0'
printf '%-24s %-10s %-16s %s\n' 'interface.pause' required implemented \
    'raven-libc/src/unistd/pause.rs: blocks in PROC_SIGRECV (kernel/src/syscall/process_ctl.rs:0x4B), returns EINTR; Symbol.map RAVEN_1.0'
printf '%-24s %-10s %-16s %s\n' 'interface.shm_open' '[SHM]' implemented \
    'raven-libc/src/compat/shm_open.rs over raven-libc-platform/src/mem/shm.rs file-backed /tmp namespace; Symbol.map RAVEN_1.0'
printf '%-24s %-10s %-16s %s\n' 'interface.shm_unlink' '[SHM]' implemented \
    'raven-libc/src/compat/shm_unlink.rs over raven-libc-platform/src/mem/shm.rs file-backed /tmp namespace; Symbol.map RAVEN_1.0'
printf '%-24s %-10s %-16s %s\n' 'interface.ualarm' 'not Issue8' waived \
    'P1f7 profile waiver (extension row); implemented as an extension in raven-libc/src/unistd/ualarm.rs over the same timer table'
printf '%-24s %-10s %-16s %s\n' 'interface.acct' 'not Issue8' waived \
    'P1f7 profile waiver; BSD process-accounting extension; owner C58'
printf '%-24s %-10s %-16s %s\n' 'interface.mincore' 'not Issue8' waived \
    'P1f7 profile waiver; ENOSYS stub PG-15 owner dave (stubs_table.rs:92)'
