<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Service supervision and restart (svcd)

`svcd` lives in `userland/init` (PID 1) and supervises the service manifests in
`/etc/services.d/*.toml`. This document describes the control surface and the client reconnect
contract introduced for the 0.1.0 restart demo (S70).

## Control scheme

The `svc:` scheme exposes the supervisor:

* `svc:` lists one directory entry per service.
* `svc:/<name>` returns one status line ` <name> <state> <handle> <restarts>\n`.
* `svc:/<name>.gen` returns one ` <generation>\n` line. The generation is the monotonic
  lifecycle counter incremented on every start and every automatic restart.
* A `CALL` on `svc:/<name>` with one byte `START` (1), `STOP` (2) or `RESTART` (3) requests the
  matching control operation. `RESTART` stops the current child and schedules the next generation.

The service name charset is `[a-z0-9_-]{1,32}`, so the `.gen` suffix never collides with a real
service name.

## Restart and generation

Each service runtime carries a `generation` counter. `Supervisor::start` and the automatic-restart
path both increment it; stale worker events (a late exit, a cancelled restart) are discarded
because the worker presents the generation it was given. `svc:/<name>.gen` publishes the current
counter so a client can detect the transition.

## Client reconnect contract

A client of a service scheme sees these defined errors:

* a call or read against a handle whose server died completes with `EIO` (or `ESTALE`);
* an open of a scheme with no registered server returns `ENOENT`.

`raven_scheme::service` provides the bounded reconnect helpers:

* `service_generation` reads `svc:/<name>.gen`;
* `service_running` reads `svc:/<name>` and checks the `running` token;
* `reopen` opens the scheme path, and only retries (up to `MAX_REOPEN_TRIES`) while the service has
  advanced past the previously observed generation and reports itself running. Any other failure
  returns the open error unchanged.

There is no silent retry loop: the retry count is bounded and the caller gets the underlying error
when the service is still down.

## Limitations

* DR-46 stands: the kernel owns pids, wait, signals and credentials; svcd is service policy only.
* Stateful recovery of open files across a restart is a follow-up. The 0.1.0 demo targets stateless
  restarts; a restarted scheme server re-registers its name and clients reopen fresh handles.
