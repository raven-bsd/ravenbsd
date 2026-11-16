<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU release profile (REL2a)

`profiles/release.toml` is the 0.1.0 QEMU release image. It extends the `shell` profile (which
already stages `/bin/brush`, the uutils slice and the base tools) and adds the release login and
network layers from `profiles/release.d/`.

## Service list

| Layer | Services / programs | Source |
|---|---|---|
| shell parent | init, logd, sysd, randd, zerod, nulld, initfsd, ramfsd, vfsd, consd, svcctl, base tools, `/bin/brush`, uutils, man | `profiles/shell.toml` + `profiles/shell.d/**` |
| login | `raven-getty`, `login`, `/etc/passwd`, `/etc/group`, `/etc/shadow`, `/etc/profile`, `/etc/motd`, getty manifest | `profiles/release.d/login.toml` (REL1c) |
| network | `raven-pcid`, `raven-devd`, `virtio-net` (`serve`), `netd` (`nic`), `resolvd`, `raven-ping`, `raven-route`, `raven-netstat`; `pcid.toml`, `devd.manifest`, `drivers.conf`, netd/resolvd manifests, `resolv.conf` | `profiles/release.d/20-net.toml` (from `profiles/server.toml`) |
| session | `rsh_enable = "NO"`, `exit_when_exits = getty` | `profiles/release.d/10-rc.toml` |

The brush shell is the `/etc/passwd` shell (`/bin/brush`); getty runs `/bin/login`, verifies the
root shadow record, prints the motd and starts brush with the `/etc/profile` environment
(`PATH=/bin:/sbin:/usr/bin:/usr/sbin`, `HOME=/root`, `TERM`).

## QEMU gate

`tests/qtest/cases/release-login.toml` boots this profile with a QEMU user-mode NIC:

```
cargo xtask qtest --case release-login --arch x86_64
```

It waits for `login:`, sends `root` / `raven`, asserts the brush prompt, `ls /bin` contains
`cat`/`ls`/`uname`, pings `10.0.2.2`, and exits with status 0.

The profile is deliberately x86_64/QEMU-only. `/bin/brush` is staged from `build/brush`; REL7 will
make the brush build arch-correct for the Pi 4 image (REL3).

## Limits

* No board claim; the Pi 4 release variant is REL3.
* The shell parent's `/etc/mounts` keeps `/etc` on the read-only initfs. netd and ping work;
  resolvd's lease-derived `/etc/resolv.conf` write is a follow-up when the release mount layout
  gains a writable `/etc`.
* The `root` / `raven` credential is the development default.
