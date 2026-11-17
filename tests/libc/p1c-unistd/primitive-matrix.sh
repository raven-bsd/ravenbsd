#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1f8 primitive/timer row matrix. This is a report artifact for the P1f9 row audit; it does not
# change the gate. Each line names the row class, the implementation state and the exact owner or
# evidence path. A row is never silently waived.

set -eu

printf '%-24s %-10s %-16s %s\n' ROW CLASS STATE OWNER/EVIDENCE
printf '%-24s %-10s %-16s %s\n' 'interface.alarm' required blocked \
    'C58 PG-17: TIMER_CREATE/TIMER_SET deliver event-queue events only (kernel/src/syscall/event.rs:289); no SIGALRM delivery'
printf '%-24s %-10s %-16s %s\n' 'interface.pause' required blocked \
    'C58 PG-7/PG-44: sigsuspend exists, production SysKernel::sig_setup/proc_kill return ENOSYS (raven-libc-signal/src/kernel.rs:200)'
printf '%-24s %-10s %-16s %s\n' 'interface.shm_open' '[SHM]' seam \
    'raven-libc-platform/src/mem/shm.rs: file-backed /tmp namespace implemented; shell/ABI export pending (frozen ABI/P1f9)'
printf '%-24s %-10s %-16s %s\n' 'interface.ualarm' 'not Issue8' waived \
    'P1f7 profile waiver; same C58 PG-17 timer/signal gap as alarm'
printf '%-24s %-10s %-16s %s\n' 'interface.acct' 'not Issue8' waived \
    'P1f7 profile waiver; BSD process-accounting extension; owner C58'
printf '%-24s %-10s %-16s %s\n' 'interface.mincore' 'not Issue8' waived \
    'P1f7 profile waiver; ENOSYS stub PG-15 owner dave (stubs_table.rs:92)'
