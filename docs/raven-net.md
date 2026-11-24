<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `raven net` (K202)

Status: **read-only `raven net if|route|status` implemented over the `net:` scheme.** The command
uses the netd control `CALL` operations, not a device, `/proc/net`, or a configuration writer.
`raven net apply` remains reserved for K198 and returns `EUNAVAILABLE`.

## Commands

```text
raven net if       # interfaces: name, state, address/prefix, gateway, counters
raven net route    # IPv4 routes: destination, gateway, interface, flags
raven net status   # compact summary; exit 0 when the scheme is readable
raven net apply    # EUNAVAILABLE (K198 owns writes)
```

Human mode prints a table. `--json` emits one NDJSON record per line with `schema: 1` and
`kind: "net.if"` / `"net.route"` (`"net.status"` for the summary). Exit codes: 0 success, 1
backend/protocol error, 2 usage. `--quiet` suppresses output.

```text
$ raven --json net if
{"schema":1,"kind":"net.if","name":"eth0","state":"up","address":"10.0.2.15","prefix":24,"gateway":"10.0.2.2","mtu":1500,"mac":"52:54:00:12:34:56","counters":{"rx_packets":2,"tx_packets":2,"rx_bytes":632,"tx_bytes":684,"rx_errors":0,"tx_errors":0,"rx_drops":0,"tx_drops":0}}

$ raven net route
DESTINATION    GATEWAY      INTERFACE  FLAGS
default        10.0.2.2     eth0       UP,GATEWAY
10.0.2.0/24    -            eth0       UP

$ raven net status
interfaces: 2 (up: 2)
default gateway: 10.0.2.2
ready: yes
```

## Implementation

* `crates/raven-cli/src/cmd/net.rs` owns the command, the `NetBackend` seam, the human/JSON
  formatting and the `EUNAVAILABLE` apply stub.
* The target backend is compiled only for `target_os = "ravenbsd"` and uses `raven-scheme`'s
  `Client::open("net:")` + `Client::call` with `raven-proto-net`'s `IfaceRequest::List`,
  `RouteList` and `Counters`, decoding `IfaceResponse::{Count,Iface,RouteInfo,Counters}`.
* `crates/raven-cli/Cargo.toml` has target-only `raven-abi`, `raven-proto-fs`,
  `raven-proto-net` and `raven-scheme` dependencies.
* `crates/raven-cli/src/commands/mod.rs` registers `&crate::cmd::net::NET_COMMAND`; the reserved
  `NET` entry is removed from `commands/reserved.rs`.
* `ifconfig`, `route` and `netstat` are unchanged compatibility commands; they still speak the
  same `net:` control channel.

## Tests

* `crates/raven-cli/src/cmd/net_ut.rs`: host tests for the human table, `net.if`/`net.route`
  JSON, status exit 0, `apply` `EUNAVAILABLE`, and usage errors.
* `tests/qtest/cases/modern-net.toml` + `tests/qtest/cases/modern-net/`: boot the release
  initfs (netd, getty/login, network tools) under QEMU user networking, add the target `raven`
  binary, log in, and assert:
  - `raven --json net if` has a ready `10.0.2.15/24` interface with gateway `10.0.2.2`;
  - `raven net route` has the `default ... 10.0.2.2` route;
  - `raven net status` exits 0.
  The pre step builds the target `raven` binary and the current `raven-init`, then repacks the
  release initfs with `/usr/sbin/raven` (and the current `/init`).

Observed (K202 rework round 1, QEMU x86_64):

```text
modern-net: pass
eth0 state=up address=10.0.2.15/24 gateway=10.0.2.2
default 10.0.2.2 eth0 UP,GATEWAY
net-status-ok
init exited (0)
```

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| Interface/route view | `ipadm`/`netstat` behaviour only | `ifconfig`/`route`/`netstat` outcomes | `ip`/`ss` outcomes only | first-party `raven net` over the existing netd scheme; no code copied |
| JSON schema | not applicable | not applicable | not applicable | Raven NDJSON `net.if`/`net.route` |

No donor code was read or copied; this is a first-party admin/network task.
