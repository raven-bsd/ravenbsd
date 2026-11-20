<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-config TUI scripted keys

`tests/qtest/cases/raven-config-tui.toml` boots the `raven-admin-test` profile and sends the
following serial dialogue. The TUI parser accepts the newline-terminated lines and shows the
stable status lines quoted below. `RAVEN_CONFIG_ROOT` points the test at writable ramfs because
the admin-test initfs is read-only; the live default is `/`.

```text
export NO_COLOR=1 RAVEN_TUI=1 RAVEN_CONFIG_ROOT=/tmp/raven-config-test
raven config --dry-run
raven config: TUI
/net
raven config: module=Network
raven-tui
raven config: dry-run plan=/tmp/raven-config-test/etc/raven/conf.d/network.toml file=absent
raven config
raven config: TUI
/net
raven config: module=Network
raven-tui
raven config: saved /tmp/raven-config-test/etc/raven/conf.d/network.toml
raven config --dry-run
raven config: TUI
                           Hostname: raven-tui
q
raven config: cancelled
exit 0
```

The `/net` line enters search and selects **Network**; the `raven-tui` line edits the hostname
(the current `raven` is replaced by the non-empty edit value) and Enter saves. The first dry-run
must leave no drop-in (`file=absent`); the real save must write only
`/tmp/raven-config-test/etc/raven/conf.d/network.toml`; the second TUI shows the saved hostname as
the current value and is cancelled with `q`. The qtest `strip_ansi` removes the frame escapes
(`NO_COLOR` selects the plain frame anyway).
