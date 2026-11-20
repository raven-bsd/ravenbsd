<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Network configuration (K198)

Status: **PROPOSED** for the release/UFS one-line inclusion after K191/K193; the static and
DHCP paths are OBSERVED in the K198 qtests on x86_64 QEMU.

## Schema

`raven-config` validates one strict `[network]` section. Unknown keys and wrong types name
the file and key; `raven config check` fails instead of ignoring them.

```toml
[network]
hostname = "raven"
mode = "dhcp" | "static"

[network.static]
address = "10.0.2.15"
prefix = 24
gateway = "10.0.2.2"
dns = ["10.0.2.3"]
```

Rules (OBSERVED in `crates/raven-config/src/network.rs` and its unit tests):

- `mode` is required and is exactly `dhcp` or `static`.
- `mode = "static"` requires `network.static.address` and `network.static.prefix`.
- `address`, `gateway` and every `dns` entry must be an IPv4 dotted quad.
- `prefix` is an integer `0..=32`.
- A gateway is optional; an empty or `0.0.0.0` gateway is rejected.
- `dns` is optional; when present it must be a non-empty array and `0.0.0.0` is rejected.
- `hostname` is `1..=64` bytes, printable, without control characters, `/`, quotes,
  backslashes or whitespace.
- A static table in `mode = "dhcp"` is validated but ignored, so a file can switch modes
  without a rewrite.

## Path and boot

- netd reads `/etc/raven/conf.d/network.toml` at startup. It tries `file:` first and
  `initfs:` second, so a ramfs `/etc` overlay does not hide the staged file.
- A missing file keeps the current image default (QEMU user-mode address plus DHCP).
- A readable but invalid file is refused with one clear `netd: network config: ...; using
  image default` line; netd continues serving loopback and the default NIC so login and the
  rest of the boot stay usable.
- `mode = "dhcp"` keeps the existing DHCP-first/QEMU behaviour; the DHCP lease still feeds
  resolvd.
- `mode = "static"` configures the address, prefix and gateway immediately, starts no DHCP
  client, and publishes a synthetic lease carrying the configured DNS list so the unchanged
  resolvd applies it.
- `hostname` is written to `sys:/hostname` once sysd is reachable; `hostname` then prints the
  configured name. There is no per-process environment hack.

## Writer for K197

`raven-config` exposes the pure authoring path:

- `NetworkConfig::render()` renders the drop-in text.
- `write_network_drop_in(root, config)` validates and atomically replaces
  `<root>/etc/raven/conf.d/network.toml` (same-directory temporary file plus rename).
- `network_from_node(node)` is the same validation used by `raven config check`.

The `raven net apply --dry-run` command is K197's caller; this ticket intentionally has no TUI.

## Tests

- `network-config` (profile `network-config-test`): static `10.0.2.15/24`, gateway
  `10.0.2.2`, DNS `10.0.2.3`, hostname `raven-static`; asserts `netd` config lines,
  `hostname`, `ifconfig`, `ping 10.0.2.2`, `df` and a clean `init exited (0)`.
- `network-config-dhcp` (profile `network-config-test-dhcp`): `mode = "dhcp"` with a
  `10.0.9.9` sentinel; asserts the DHCP lease and that the sentinel never appears.
- Invalid drop-ins are covered by unit tests for both the authoring validator and the netd
  boot parser; a boot with an invalid file prints the refusal line and keeps the default
  bootstrap.

Profiles extend `release` and stage only
`/etc/raven/conf.d/network.toml`; the final release/UFS fragment is a one-line profile
inclusion after K191/K193 and does not edit their active paths.

## Donor reconciliation

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| Network config | SMF/ipadm properties (behaviour) | FreeBSD `rc.conf`/`ifconfig`, NetBSD `ifconfig.*` (outcomes) | NetworkManager/systemd-networkd (outcomes) | Strict TOML; one validated drop-in; no shell evaluation. |
| DHCP/static | DHCP client behaviour | dhclient/ifconfig outcomes | dhcpcd outcomes | Keep the existing DHCP core; add a static mode with explicit gateway/DNS and no DHCP client. |
| Hostname | `nodename`/SMF property | `hostname`/rc.conf outcome | hostnamed outcome | netd applies the configured name through `sys:/hostname`; `hostname` reads it. |

No donor code was copied or translated; this is first-party Rust over the existing RavenBSD
`raven-config`, `netd` and `sysd` interfaces.
