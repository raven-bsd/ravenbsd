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
  Users (add)            Mode: dhcp
  Users: root            Address: (not set)
  Users: raven           Prefix: 24
  Services              Gateway: (not set)
  Logging                DNS: (not set)
  Theme                  [ Save ]  [ Cancel ]
  About
status: ...
```

Modules: **Network** and the **Users** forms (writable), **Services**, **Logging**, **Theme**
and **About** (read-only current values/help; K198/K76 own their apply paths). The Users add
form and one form per account are generated from the files-backed account model; see
[`docs/users.md`](users.md). The registry is data-driven in `crates/raven-tui/src/model.rs`;
adding a module is a registry entry, not a new renderer.

## Keys

| Key | Action |
|---|---|
| `/` or any printable character in the list | enter/append the module search filter |
| Up/Down | move the module selection (list) or the field selection (form) |
| Enter | open the selected module; in the form, validate and save |
| Tab / Down | next form field |
| Up | previous form field |
| Left/Right or the first letter | cycle a choice (`dhcp`/`static`, `no`/`yes`) |
| Backspace | delete one character from a text field |
| Ctrl-S | validate and save (same as Enter in the form) |
| Esc | leave the form (back to list); from the list, cancel |
| q | cancel from the list (not while editing a text field) |

The input parser accepts a canonical line (characters followed by newline) and raw ANSI arrows,
so both a normal shell and the serial qtest can drive it. `NO_COLOR`, `TERM=dumb` and
`--color never` select the plain layout; otherwise ANSI is used only for styling and clearing.

## Raw terminal and color

On the RavenBSD console the binary puts the terminal in raw mode through the first-party
`crates/raven-tty` guard. The console is the `tty:console` scheme, so the guard saves
`consd`'s `GET_MODE` and applies `SET_MODE(raw, echo off)`; a POSIX tty uses the
`tcgetattr`/`tcsetattr` path instead. `ICANON`, `ECHO`, `ISIG` and the other line-discipline
flags are cleared and the 8-bit data path is selected. The guard restores the saved settings on
every normal exit and on `SIGHUP`/`SIGINT`/`SIGQUIT`/`SIGTERM`. When the console is unavailable
the binary keeps the canonical line parser, so the scripted qtests and non-TTY invocations still
work.

Color mode draws a bordered two-pane frame, a bold/dim header, a reverse-video selected module
row, a marked selected field, a colored status line and a dim key-help footer. The `>` selection
marker stays in every mode, so color is never the only signal. The serial profile's
`TERM=vt100` is enough for the ANSI 16/256 SGR sequences; `NO_COLOR`, `TERM=dumb` and
`--color never` disable styling, while `--color always` forces it.

The raw-key gate (`tests/qtest/cases/raven-tui-keys.toml`) copies the stock demo qcow2 to a
case-specific image, sends Down/Tab/Enter/Esc without a newline, checks the selected module and
form field change, adds `consoleuser` through the raw console, saves and reads back
passwd/shadow/group and the home, and logs in with the new password. The canonical `/users\n`
line path and `--color always` are run in the same case.

The scheme-backed login console can report `isatty(0) == false` even when the interactive profile
is loaded. `RAVEN_TUI=1` is the explicit override for that console and for the scripted qtest;
`RAVEN_TUI_RAW=1` opts a scripted session into the console path instead of the canonical stdin
fallback. `brush -c` does not load the login profile, so non-interactive scripts keep the usage
path unless the override is explicitly set.

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

## Users module (K229)

The Users forms edit the real account database: `/etc/passwd`, `/etc/shadow`, `/etc/group` and
`/etc/raven/autologin.toml`. The add form takes username, uid/gid, home, shell, password and
confirmation (both masked), console autologin and, on an existing account, a delete choice.
Passwords are hashed with `raven-auth` SHA-512-crypt `$6$`; the shadow file is written atomically
with mode 0600. A save creates `/home/<name>` with the account's uid/gid and mode 0755 (the
admin runs as root), preserves root/uid 0 and leaves unrelated accounts intact.

`--dry-run` on a Users form names the `/etc/passwd` target, reports `file=present` when it
exists, and prints the plan with `password = "<set>"` or `"<unchanged>"`; it never prints a
secret. The scriptable surface is `raven users list|show|check` (JSON with `--json`). Details,
the base `raven` account and the musicbox/TUI-shell pattern are in [`docs/users.md`](users.md).

## Tests

* Host state-machine/model tests: `cargo nextest run -p raven-tui -p raven-config` (search,
  selection, editing, validation, atomic writes, Users add/edit/delete, the `$6$` hash, dry-run,
  read-only modules, non-TTY guard) and `cargo nextest run -p raven` (the binary invocation
  guard and `users list/show/check` JSON). `cargo nextest run -p login -p raven-getty` covers
  multi-account authentication, the passwd shell and console autologin.
* `raven-users` (`tests/qtest/cases/raven-users.toml`): boots the one-disk UFS demo, adds
  `musicbox` with a stub `/bin/tui-demo` through the TUI, verifies passwd/shadow/group, the home,
  the `$6$` hash and the autologin file, authenticates with `/bin/login`, runs the passwd shell
  locally, and checks `raven users list/show/check` JSON. The `ssh musicbox@` proof remains K195.
* `raven-tui-keys` (`tests/qtest/cases/raven-tui-keys.toml`): boots a case-specific copy of the
  stock UFS demo, sends raw Down/Tab/Enter/Esc bytes without a newline, asserts the selected
  module (`Network` -> `Users (add)` -> `Users: root`) and the form field (`UID`) change, adds
  `consoleuser` on the raw console, saves and reads back passwd/shadow/group and the home, logs
  in, then re-runs the canonical `/users\n` line path and the `--color always` path.
  `cargo nextest run -p raven-tty` covers the literal NetBSD `cfmakeraw` flag port and the
  non-TTY rejection.
* `raven-config-tui` qtest (`tests/qtest/cases/raven-config-tui.toml`): boots the
  `raven-admin-test` profile, sets `NO_COLOR=1`, `RAVEN_TUI=1` and
  `RAVEN_CONFIG_ROOT=/tmp/raven-config-test`, drives `raven config --dry-run` and the real save
  with the scripted keys in `tests/fixtures/raven-tui/README.md`, checks the dry-run wrote
  nothing, and reads the saved drop-in back.
* `modern-admin` remains the non-TUI acceptance (`raven config check`).

## Release/UFS staging (K209)

The release profile extends `shell` and gets a target-built `raven` at `/usr/sbin/raven` from
`profiles/release.d/90-raven.toml`. The artifact is produced by
`tests/fixtures/modern-admin/build.sh` (a target build, not the host binary); the
`raven-release` qtest runs that builder before the image. `/usr/sbin` is in the root PATH
(`/etc/profile`), so `raven ...` and `/usr/sbin/raven ...` are equivalent.

The release image's root is the read-only initfs: a default `/etc/raven/conf.d` save fails
cleanly with `Read-only file system` and returns to the login prompt. The qtest exercises that
clean failure, then uses the documented `RAVEN_CONFIG_ROOT=/tmp/raven-release-test` (ramfs) for
the atomic drop-in proof and reads the saved hostname/mode back. The live default remains `/`
on a writable UFS root.

The UFS payload (`qemu-ufs`, which extends `shell`) gets the same target-built binary from
`profiles/qemu-ufs.d/90-raven.toml`, plus the K198 default drop-in at
`/etc/raven/conf.d/network.toml`. No `shell.d` fragment is used: a `/usr/sbin/raven` staging there
would collide with the fixture-only `profiles/raven-admin-test.d/20-raven.toml`, and the profile
merge rejects duplicate paths. The UFS root is writable, so `raven config --dry-run` reports
`file=present` there and a validated save lands on the UFS root.

`raven-release` (`tests/qtest/cases/raven-release.toml`) boots the release profile, logs in,
asserts `/usr/sbin/raven` is executable, runs `raven net if/route/status` and `raven --json status`,
then drives the dry-run, the clean read-only failure and the test-root save, and reads the
drop-in back. `raven-ufs` (`tests/qtest/cases/raven-ufs.toml`) builds and boots the one-disk
demo qcow2, logs in, asserts `/usr/sbin/raven`, `raven net if/route/status` and `raven --json status`,
runs `raven config check`/`get`, then drives the dry-run (`file=present`) and a real atomic save
on UFS and reads the saved drop-in back. `release-login`, `modern-admin` and `modern-net` remain
green.

## Donor reconciliation

| Topic | Illumos | BSD | Linux | Decision |
|---|---|---|---|---|
| Admin TUI | SMF/`svccfg` command-driven; no TUI | BSD `sysinstall`/`bsdconfig` form behaviour | `nmtui`/`raspi-config` outcomes | first-party state machine and ANSI renderer; no copied code |
| Config writes | SMF property groups (behaviour) | `rc.conf`/TOML drop-ins | NetworkManager/TOML outcomes | dedicated validated drop-in, atomic write, no eval |
| Searchable forms | not applicable | not applicable | not applicable | first-party filter and form model |

No donor code was copied; the visual language is first-party and inspired by the Crush/glamour
two-pane layout.
