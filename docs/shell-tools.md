<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Shell-visible first-party tools (K167)

Status: **fixed and verified from the shell.** The release/shell `/bin` first-party tools are
linked statically; `getty`/`login` remain dynamic so the loader/libc startup gate stays intact.

## Why

The release/shell profiles link `/bin` programs as dynamic PIE against `libc.so.1` and
`ld-elf.so.1`. The shell runs commands through libc `posix_spawn`, which uses
`raven-spawn`'s `spawn_image_with_fdmap`; that path refuses an interpreter image. The result was:

```text
raven# ls -l /bin/uname
-rwxr-xr-x 1 root root 12504 Jan  1 00:00 /bin/uname
raven# /bin/uname -a
error: command not found: /bin/uname
raven# /bin/ps
error: command not found: /bin/ps
raven# wc -l /etc/passwd
1 /etc/passwd
```

`wc`/`ls` are static uutils, so they worked. `/sbin` services are already static by the initfs
path rule, but the shell-visible `/bin` tools were not.

## Fix

`xtask/src/initfs.rs` now classifies the shell-visible `/bin` packages as static before the
profile build:

```rust
const STATIC_SHELL_PACKAGES: &[&str] = &[
    "man", "raven-date", "raven-hostname", "raven-ps", "raven-rsh", "raven-sleep",
    "raven-svcctl", "raven-uname",
];

fn program_is_static(program: &Program) -> bool {
    !program.path.starts_with("/bin/") || STATIC_SHELL_PACKAGES.contains(&program.package.as_str())
}
```

`raven-getty` and `login` are deliberately not in the list: they stay dynamic, keeping the
release loader/libc gate meaningful. `/sbin` programs (`dmesg`, `mount`, `umount`, `ifconfig`,
`ping`, `route`, `netstat`, services) remain static by path. uutils (`ls`, `wc`, `cat`, `id`, ...)
are already static files staged from `build/{arch}/uutils/`.

## Inventory (release/shell profiles)

| Tool | Path | Package | Old link | New link |
|---|---|---|---|---|
| uname | `/bin/uname` | `raven-uname` | dynamic PIE | static EXEC |
| ps | `/bin/ps` | `raven-ps` | dynamic PIE | static EXEC |
| date | `/bin/date` | `raven-date` | dynamic PIE | static EXEC |
| hostname | `/bin/hostname` | `raven-hostname` | dynamic PIE | static EXEC |
| sleep | `/bin/sleep` | `raven-sleep` | dynamic PIE | static EXEC |
| svcctl | `/bin/svcctl` | `raven-svcctl` | dynamic PIE | static EXEC |
| man | `/bin/man` | `man` | dynamic PIE | static EXEC |
| rsh | `/bin/rsh` | `raven-rsh` | dynamic PIE | static EXEC |
| getty | `/bin/getty` | `raven-getty` | dynamic PIE | **dynamic (gate)** |
| login | `/bin/login` | `login` | dynamic PIE | **dynamic (gate)** |
| dmesg/mount/umount/ifconfig/ping/route/netstat | `/sbin/*` | first-party | static | static |
| ls/wc/cat/id/... | `/bin/*` | uutils | static | static |

`env` is not staged in the shell/release profiles; `id` is the static uutils slice. No new tool
was added.

## Tests

`tests/qtest/cases/base-tools.toml` and `tests/qtest/cases/release-login.toml` now run the tools
and assert the output, not the echoed command:

* `uname -a` must match `RavenBSD \S+ 0\.1 RavenBSD 0\.1 \(\S+\) #\d+: .+ x86_64`.
* `ps -e` must print `PID PPID UID GID CMD` and at least one process row.
* `release-login` also forbids `command not found`, so a future dynamic staging regression fails
  the gate instead of passing silently.

## Evidence (x86_64, release initfs)

```text
$ /usr/bin/time -p timeout 360 cargo xtask initfs --arch x86_64 --profile release
real 254.74
exit 0
build/x86_64/initfs.img 43069440 bytes
```

Artifact shapes:

| Binary | ELF type | INTERP |
|---|---|---|
| `raven-uname`, `raven-ps`, `raven-date`, `raven-hostname`, `raven-sleep`, `raven-svcctl`, `man`, `raven-rsh` | `EXEC` | none |
| `raven-getty`, `login` | `DYN` (PIE) | `/libexec/ld-elf.so.1` |

`timeout 120 cargo xtask qtest --case base-tools --arch x86_64` → pass, with the transcript
showing:

```text
RavenBSD raven 0.1 RavenBSD 0.1 (BASE) #1: <build> x86_64
PID PPID UID GID CMD
```

The release-login run is recorded in `tickets/tim/reports/K167.md` after the clean-image build.

## Donor reconciliation (DR-52)

Build and profile staging only: no donor code was read or copied, no C, no CDDL/GPL. The static
link path is the existing first-party `x86_64-unknown-none` user build; uutils and disk tools
already used it.
