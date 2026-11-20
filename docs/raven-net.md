<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `raven net` (K202)

Status: **BLOCKED — the command registration and the target scheme backend are outside K202's
Owns.** No product code was changed; this document records the required wiring and the planned
command surface.

## Contract

`raven net if` lists interfaces (name, state, address/prefix, gateway, counters), `raven net
route` lists IPv4 routes, and `raven net status` prints a compact one-screen summary. Human mode
is a table; `--json` emits NDJSON with `schema: 1` and `kind: "net.if"` / `kind: "net.route"`.
`raven net apply` is reserved for K198 and returns `EUNAVAILABLE` until then.

Data comes from the existing `net:` scheme control channel (the same channel
`ifconfig`/`route`/`netstat` use). No config writes, no device opens, no `/proc/net`, no duplicate
network stack.

## Why the ticket is blocked

1. **Command table.** `crates/raven-cli/src/commands/mod.rs` owns the `COMMANDS` table and
   currently registers `&reserved::NET` (`commands/reserved.rs`), whose `run` always returns
   `EUNAVAILABLE`. K202's Owns are `crates/raven-cli/src/cmd/net*` and `cmd/mod.rs` only; the
   table and the reserved `NET` entry are in `crates/raven-cli/src/commands/`, which is not owned.
   A `net` module can be declared in `cmd/mod.rs`, but it cannot be selected without the table
   row.
2. **Target scheme access.** The target `raven` binary's only scheme seam is
   `userland/bin/raven/src/admin.rs` (`Scheme::call`, `RtScheme` over `raven-scheme`); it
   dispatches only `status`, `doctor` and `svc`. `net` falls through to `raven_cli::run`, which
   has no scheme client. `crates/raven-cli` has no `raven-scheme` or `raven-proto-net` dependency,
   and the `net:` control file is `CALL`-only (`userland/servers/netd/src/netd.rs` returns
   `EINVAL` for a control `READ`), so `std::fs` cannot read it. The target wiring needs
   `userland/bin/raven/src/admin.rs` and either `userland/bin/raven/Cargo.toml` or
   `crates/raven-cli/Cargo.toml`, all outside the Owns.

## Required Owns (choose one)

**Option A (recommended): direct scheme backend in the target `raven` binary.**

Add to K202's Owns:

* `crates/raven-cli/src/commands/mod.rs` — replace `&reserved::NET` with
  `&crate::cmd::net::NET_COMMAND`.
* `crates/raven-cli/src/commands/reserved.rs` — drop the reserved `NET` entry.
* `userland/bin/raven/src/admin.rs` — add `"net"` to the dispatch, using the existing
  `Scheme::call` / `RtScheme` to open `net:` and issue `IfaceRequest::List`, `RouteList` and
  `Counters`; decode with `raven-proto-net`.
* `userland/bin/raven/Cargo.toml` — add `raven-proto-net` to the `target_os = "ravenbsd"`
  dependencies.

**Option B: target backend inside `raven-cli`.**

Add `crates/raven-cli/Cargo.toml` (target-only `raven-scheme`, `raven-proto-net`,
`raven-proto-fs`), a `cfg(target_os = "ravenbsd")` backend under `cmd/net/`, and the
`commands/mod.rs` table change. This avoids `raven` edits but adds target dependencies to the host
crate.

The `modern-net` qtest can live entirely under `tests/qtest/cases/modern-net*` (Owns): a pre step
builds the existing `tests/fixtures/modern-admin/build.sh` target `raven`, builds the release
initfs (which already has netd, ifconfig, route and netstat), repacks `/bin/raven` into it, and
runs the guest assertions. No profile edit is required.

## Planned command surface

```text
raven net if
NAME  STATE  ADDRESS       GATEWAY   RX_PACKETS  TX_PACKETS
eth0  up     10.0.2.15/24  10.0.2.2  ...
lo    up     127.0.0.1/8   -

raven net route
DESTINATION  GATEWAY   INTERFACE  FLAGS
default      10.0.2.2  eth0       UP,GATEWAY
10.0.2.0/24  -         eth0       UP

raven net status
interfaces: 2 (up: 2)
default gateway: 10.0.2.2
ready: yes
```

JSON, one record per line:

```json
{"schema":1,"kind":"net.if","name":"eth0","state":"up","address":"10.0.2.15","prefix":24,"gateway":"10.0.2.2","mtu":1500,"mac":"…","counters":{"rx_packets":0,"tx_packets":0,"rx_bytes":0,"tx_bytes":0,"rx_errors":0,"tx_errors":0,"rx_drops":0,"tx_drops":0}}
{"schema":1,"kind":"net.route","destination":"default","gateway":"10.0.2.2","interface":"eth0","flags":["UP","GATEWAY"]}
```

Exit codes: 0 success, 1 backend/protocol error, 2 usage. `net apply` returns `EUNAVAILABLE`
(exit 1) until K198.

## Tests planned

* Host: a `NetBackend` fake with `eth0` `10.0.2.15/24`, gateway `10.0.2.2`, a route and counters;
  assert the human table contains the address/prefix and gateway, the JSON lines have
  `schema:1`/`kind:"net.if"`/`kind:"net.route"`, `status` exits 0, `apply` is `EUNAVAILABLE`, and
  a bad subcommand is exit 2.
* `modern-net` qtest (release QEMU user network): `raven --json net if` contains a ready
  `10.0.2.15/24` record; `raven net route` contains the `10.0.2.2` gateway; `raven net status`
  exits 0. Assert the existing `ifconfig`/`route`/`netstat` output is unchanged.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| Interface/route view | `ipadm`/`netstat` behaviour only | `ifconfig`/`route`/`netstat` outcomes | `ip`/`ss` outcomes only | first-party `raven net` over the existing netd scheme; no code copied |
| JSON schema | not applicable | not applicable | not applicable | Raven NDJSON `net.if`/`net.route` |

No donor code is read or copied; this is a first-party admin/network task.
