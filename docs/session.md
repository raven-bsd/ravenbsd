<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Console session and logout policy

Status: **K234.** The UFS demo keeps a console session alive across a normal shell `exit`; the
release qtest remains one-shot for `test-exit=1`.

## 1. qemu-ufs demo

`profiles/qemu-ufs.d/10-session.toml` now stages the demo session files instead of the release
ones:

- `profiles/qemu-ufs.d/getty.manifest` — `getty` has `restart = "always"`, `restart_delay_ms = 200`,
  `max_restarts = 1000` and needs `consd`, `vfsd`, `ramfsd`, `initfsd`;
- `profiles/qemu-ufs.d/svcd.conf` — no `exit_when_exits = getty`; svcd stays alive while getty is
  respawned.

The service file uses the established `.manifest` extension: every `.toml` under
`profiles/qemu-ufs.d/` is a profile fragment, while `.manifest` is staged as the run-time
`/etc/services.d/getty.toml` (same pattern as the release and ufs-demo profiles).

The bootstrap initfs must carry the same demo `svcd.conf`
(`profiles/qemu-ufs-boot.d/40-rc.toml`): svcd reads `/etc/svcd.conf` once, before it pivots
the root to UFS, and never re-reads it. Staging the release file there would keep
`exit_when_exits = getty` active even though the pivoted root has the demo file. The service
manifests are re-read after the pivot, so the UFS-root `getty` manifest supplies
`restart = "always"`.

A normal `exit` from the brush shell is a success; the supervisor restarts getty, and the console
prints `login:` again. A second login is accepted. The service graph keeps the resident
`vfsd`/`consd`/`ramfsd`/`initfsd` services; getty is the only service with `always`.

The release profile is unchanged: `profiles/release.d/svcd.conf` still has
`exit_when_exits = getty` and `profiles/release.d/login.service.manifest` stays `on-failure`, so
`release-login` still exits one-shot with `test-exit=1`.

## 2. Bounded evidence

`tests/qtest/cases/ufs-logout.toml` boots `qemu-ufs` **without** `test-exit`, logs in, runs
`exit`, waits for a second `login:`, logs in again, exits again and waits for a third `login:`.
It ends with `expect_exit = "timeout"` (K235 will provide a clean shutdown); it never fakes an
init exit.

```sh
cargo xtask qtest --case ufs-logout --arch x86_64
cargo xtask qtest --case release-login --arch x86_64
```

## 3. Donor reconciliation

This is a session-policy configuration change, not new platform code. The donor map is recorded
with exact paths/functions; no donor code is copied.

| Topic | Illumos (CDDL, behaviour only) | BSD (permissive) | Linux (read-only outcome) | Decision |
|---|---|---|---|---|
| getty respawn / session lifetime | `illumos-gate/usr/src/cmd/svc/startd/restarter.c` @ `586678ac`: `restarter_insert_inst` (316), `restarter_instance_update_states` (837), `stop_instance` (1032), `start_instance` (1508), `maintain_instance` (1597); restart policy is an operator choice, not an exit of the master restarter | NetBSD @ `36eddebcc`: `sbin/init/init.c` `read_ttys` (1171), `start_getty` (1270, `GETTY_SPACING` 92), `collect_child` (1419), `multi_user` (1487), `clean_ttys` (1528); `libexec/getty/main.c` `main` (183), login exec (386), `prompt` (565); `usr.bin/login/login.c` `main` (136), `execlp` shell (721). BSD-2/3 | systemd `src/core/service.c` (upstream `main`): `service_enter_dead` (2505), `service_restart_usec_next` (405), `SERVICE_RESTART_ALWAYS` (2465); a successful service exit with `Restart=always` is restarted | Port the NetBSD ttys/getty respawn policy as first-party profile/service configuration. No Illumos/Linux code copied; no C. |

The NetBSD `start_getty` spacing (`GETTY_SPACING = 5`) and fork/exec pattern are the behavioural
reference for the `restart_delay_ms = 200` bound; the implementation is the existing first-party
supervisor restart policy. No ad-hoc session plumbing was added.

## 4. Non-goals

No SSH/network change, no `/bin/raven` path change (K236), no release `test-exit` change and no
Pi 4 claim.
