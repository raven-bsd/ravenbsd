<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `raven config` TUI

`raven config` with no subcommand on a TTY opens a first-party two-pane TUI. The left pane is a
searchable module list; the right pane is the module summary, current values, editable fields,
validation errors and the Save/Cancel actions. On a non-TTY (or with `--json`/`--quiet`) the
existing scriptable surface remains `raven config check`, `show` and `get <key>`; `raven config`
with no subcommand exits 2 with a usage message and never tries to draw.

## Layout and modules

```
Modules                 Network  (Hostname, DHCP or static IPv4 address)
> Network                Hostname: raven
  Services              Mode: dhcp
  Logging                Address: (not set)
  Theme                  Prefix: 24
  About                  Gateway: (not set)
                         DNS: (not set)
                         [ Save ]  [ Cancel ]
status: ...
```

Modules: **Network** (the writable form), **Services**, **Logging**, **Theme** and **About**
(read-only current values/help in this ticket; K198/K76 own their apply paths). The registry is
data-driven in `crates/raven-tui/src/model.rs`; adding a module is a registry entry, not a new
renderer.

## Keys

| Key | Action |
|---|---|
| `/` or any printable character in the list | enter/append the module search filter |
| Up/Down | move the module selection (list) or the field selection (form) |
| Enter | open the selected module; in the form, validate and save |
| Tab / Down | next form field |
| Up | previous form field |
| Left/Right | toggle the `mode` choice (`dhcp`/`static`) |
| Backspace | delete one character from a text field |
| Ctrl-S | validate and save (same as Enter in the form) |
| Esc | leave the form (back to list); from the list, cancel |
| q | cancel from the list (not while editing a text field) |

The input parser accepts a canonical line (characters followed by newline) and raw ANSI arrows,
so both a normal shell and the serial qtest can drive it. `NO_COLOR`, `TERM=dumb` and
`--color never` select the plain layout; otherwise ANSI is used only for styling and clearing.

The scheme-backed login console can report `isatty(0) == false` even when the interactive profile
is loaded. `RAVEN_TUI=1` is the explicit override for that console and for the scripted qtest;
`brush -c` does not load the login profile, so non-interactive scripts keep the usage path unless
the override is explicitly set.

## Save, dry-run and validation

`Ctrl-S`/Enter builds a plan from the form. Validation rejects an empty/invalid hostname, an
unknown mode, and, for `mode = "static"`, a non-IPv4 or zero address/gateway, a prefix outside
1-32, or an invalid DNS entry. A validation failure keeps the TUI open, marks the field error and
writes nothing.

`--dry-run` prints the exact plan, the target path and whether that file currently exists
(`file=absent`/`file=present`), and writes nothing. A validated save is
atomic: the bytes go to a temporary file in the same directory, are synced, chmod 0644 and renamed
to the dedicated drop-in. No other file, including `/etc/raven/raven.toml`, is rewritten or
deleted. The live default root is `/`; `RAVEN_CONFIG_ROOT` is the explicit test/console root
override (the qtest uses `/tmp/raven-config-test` because the admin-test initfs is read-only).
The schema is the K198 contract:

```toml
# Managed by raven config (K197).
[network]
hostname = "raven"
mode = "static"
[network.static]
address = "10.0.2.15"
prefix = 24
gateway = "10.0.2.2"
dns = ["10.0.2.3"]
```

`mode = "dhcp"` omits the `[network.static]` block. The writer lives in
`crates/raven-tui/src/model.rs`; K198 owns netd's apply and the strict `raven-config` schema. The
form does not hard-code a shell command, `eval` or a vendor TUI framework.

## Tests

* Host state-machine/model tests: `cargo nextest run -p raven-tui` (search, selection, editing,
  validation, atomic write, dry-run, read-only modules, non-TTY guard) and
  `cargo nextest run -p raven` (the binary invocation guard).
* `raven-config-tui` qtest (`tests/qtest/cases/raven-config-tui.toml`): boots the
  `raven-admin-test` profile, sets `NO_COLOR=1`, `RAVEN_TUI=1` and
  `RAVEN_CONFIG_ROOT=/tmp/raven-config-test`, drives `raven config --dry-run` and the real save
  with the scripted keys in `tests/fixtures/raven-tui/README.md`, checks the dry-run wrote
  nothing, and reads the saved drop-in back.
* `modern-admin` remains the non-TUI acceptance (`raven config check`).

## Release/UFS staging (K209)

The release profile extends `shell` and gets a target-built `raven` at `/bin/raven` from
`profiles/release.d/90-raven.toml`. The artifact is produced by
`tests/fixtures/modern-admin/build.sh` (a target build, not the host binary); the
`raven-release` qtest runs that builder before the image. The release/UFS `/etc` is writable, so
the TUI saves `/etc/raven/conf.d/network.toml`; the file is staged with the K198 default and the
qtest reads back the saved hostname/mode. The bare shell base and the `raven-admin-test` fixture
keep their own staging entry; a `shell.d` duplicate would collide on `/bin/raven` with that test
fragment. The read-only initfs failure path uses the documented `RAVEN_CONFIG_ROOT` test root and
does not crash login.

`raven-release` (`tests/qtest/cases/raven-release.toml`) boots the release profile, logs in,
asserts `/bin/raven` is executable, runs `raven net if/route/status` and `raven --json status`,
then drives the dry-run and the real UFS save and reads the drop-in back. `release-login`,
`modern-admin` and `modern-net` remain green.

## Donor reconciliation

| Topic | Illumos | BSD | Linux | Decision |
|---|---|---|---|---|
| Admin TUI | SMF/`svccfg` command-driven; no TUI | BSD `sysinstall`/`bsdconfig` form behaviour | `nmtui`/`raspi-config` outcomes | first-party state machine and ANSI renderer; no copied code |
| Config writes | SMF property groups (behaviour) | `rc.conf`/TOML drop-ins | NetworkManager/TOML outcomes | dedicated validated drop-in, atomic write, no eval |
| Searchable forms | not applicable | not applicable | not applicable | first-party filter and form model |

No donor code was copied; the visual language is first-party and inspired by the Crush/glamour
two-pane layout.
