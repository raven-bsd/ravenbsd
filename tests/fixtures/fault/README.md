<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# S71 fault fixtures

`tests/fixtures/fault/` is one self-contained initfs fixture used by `fault-restart` and
`fault-reconnect` on x86_64 and aarch64:

* `src/main.rs` — `raven-fault`:
  * `worker` registers the `frt:` scheme; a read on `frt:/poison` calls `raven_rt::exit(1)`
    inside the handler, so the client's in-flight `READ` has no reply and the process dies.
  * `client` (in-flight/reconnect) reads `svc:/frt.gen`, dies on the poison read, checks the
    `EIO`/`EPIPE`, waits for the generation change, checks the stale handle still fails and
    reopens through `raven_scheme::service::reopen`.
  * `restart` (kill matrix) reads through a live handle, sends `svc:` RESTART (during restart),
    checks the stale handle, reopens, sends STOP (idle), then START and reopens again.
* `worker.toml` — the restartable `frt` worker (`readiness = "scheme"`, `restart = "on-failure"`).
* `client.toml` — the in-flight/reconnect client (`args = ["client"]`).
* `restart-client.toml` — the kill-matrix client (`args = ["restart"]`).
* `svcd.conf` — `exit_when_settled = true`.
* `build.sh` — builds `raven-init` and `raven-fault`, then packs
  `build/fixtures/fault/initfs.img` (reconnect) and `build/fixtures/fault/initfs-restart.img`
  (kill matrix).

The x86_64-only release-image matrix (`netd` and `vfsd` through `svcctl`) stays in
`tests/qtest/cases/fault-restart-release.toml`; `blockd`/`fsd-ufs` live in their own test
profiles.
