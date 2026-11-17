# procd (removed)

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

Status: **REMOVED 2026-11-17 by DR-46.**

The kernel owns process semantics: pids, parentage, process groups/sessions, credentials, signal
delivery, exit status, `wait`, spawn and `execve`. There is no userspace process server, no `proc:`
scheme and no `raven-proto-proc` crate.

`raven-rt`/`raven-libc-proc` build the ELF image, argv/envp and spawn attributes, then call the
kernel `PROC_*`/`MAP`/`THREAD_CREATE` interface directly. An optional per-profile job/session
manager may exist later for policy or UX, but it must not own pids, `wait`, signals or credentials.

The former P2e/P2n implementation is preserved in git history only.
