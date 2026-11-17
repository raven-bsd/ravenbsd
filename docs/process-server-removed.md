<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Userspace process server (removed)

Status: **REMOVED 2026-11-17 by DR-46.**

The kernel owns process semantics: pids, parentage, process groups/sessions, credentials, signal
delivery, exit status, `wait`, spawn and `execve`. There is no userspace process server, no
process-server scheme and no dedicated process-protocol crate.

`raven-rt`/`raven-libc-proc` build the ELF image, argv/envp and spawn attributes, then call the
kernel `PROC_*`/`MAP`/`THREAD_CREATE` interface directly. An optional per-profile job/session
manager may exist later for policy or UX, but it must not own pids, `wait`, signals or credentials.

The former P2e/P2n implementation is preserved in git history only.

## Kernel replacement: command names (P2o)

The kernel `Process` record carries `command` (`Mutex<String>`), separate from the image `path`:

* PID 1 starts with the init path; `PROC_CREATE` starts as `child`; `PROC_FORK` copies the
  source command; `PROC_EXEC` replaces it with argv[0] (path fallback).
* `PROC_SET_COMMAND` (raw `0x45`, `proc_set_command(proc: Handle, name: &[u8]) -> 0`) sets the
  name for `SELF_PROC` or a process handle with `ADMIN`. The name is UTF-8 without NUL, capped
  at 256 bytes on a UTF-8 boundary.
* `raven_rt::spawn::spawn` and `spawn_with_attributes` call `PROC_SET_COMMAND` on the child
  before `THREAD_CREATE`, so kernel-spawned children are named without a process server.
* `PROC_PID_INFO`/`PROC_SELF_INFO` keep the eight-word P2a payload. A caller passing `count` in
  `9..=41` gets the NUL-terminated command at byte offset 64; the command is truncated at a
  UTF-8 boundary inside the buffer. Counts outside `8..=41` are `EINVAL`.
* `ps -e` should request 41 words and read the command from word 8 onward. A process that execs
  shows the new argv[0]; a fork child shows the copied command; an exited process is `ESRCH`
  from `PROC_PID_INFO`.
