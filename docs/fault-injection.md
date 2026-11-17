<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# S71 fault-injection harness for server death

The harness kills a server at controlled protocol steps and proves that the kernel survives, the
client gets a defined error, the service restarts with a new generation, and a bounded reopen
brings the client back without a stale-handle hang.

## Driver

```sh
sh scripts/fault-injection.sh --arch x86_64              # fault-restart + fault-reconnect
sh scripts/fault-injection.sh --arch aarch64             # same two cases on aarch64
sh scripts/fault-injection.sh --profile release          # x86_64 release matrix (netd, vfsd)
sh scripts/fault-injection.sh --case fault-reconnect     # one case
sh scripts/fault-injection.sh --soak 24h                 # repeat the selected set for 24 h
sh scripts/fault-injection.sh --soak 86400               # the same budget in seconds
cargo xtask qtest --case fault-restart --arch aarch64    # the qtest the driver runs
```

`--soak` takes a duration: `<n>` (seconds), `<n>s`, `<n>m`, `<n>h` or `<a>h<b>m` (`1h30m`). Any
other value is a usage error (exit 2) that names the offending value, and a budget that would run
zero iterations (`--soak 0s`) is rejected rather than reported as a pass: a soak with no runs is
never green. `--self-test` runs the duration-parser checks without starting a QEMU case, and
`tests/scripts/check-fault-injection-duration.sh` wraps the parser, the usage-error paths and the
zero-run guard as a host check (no QEMU):

```sh
sh tests/scripts/check-fault-injection-duration.sh
```

## Cases

| Case | Image | Kill points | Scope |
|---|---|---|---|
| `fault-restart` | self-contained `frt` worker | after reply, during `svc:` RESTART, idle `svc:` STOP/START | x86_64 and aarch64 |
| `fault-reconnect` | self-contained `frt` worker | **in-flight** request (the worker dies inside the read handler) | x86_64 and aarch64 |
| `fault-restart-release` | `release` profile | after reply, during restart, idle stop/start on `netd` and `vfsd` through `svcctl` | x86_64 only |

`fault-restart` and `fault-reconnect` share `tests/fixtures/fault/`, which builds two initfs
images from one binary: `initfs-restart.img` (`client` mode `restart`) and `initfs.img` (`client`
mode `client`). `raven-init` is PID 1; the `frt` worker is a restartable `scheme` service and the
driver is a one-shot `exit` service.

`fault-restart-release` boots the release image, logs in as root, and drives `svcctl`:

| Step | Command | Expected |
|---|---|---|
| baseline | `svcctl list` | `netd running <n>`, `vfsd running <n>` |
| restart | `svcctl restart netd` | `netd stopped <n>`, `netd running <n>` |
| stop | `svcctl stop netd` | `netd stopped <n>` |
| start | `svcctl start netd` | `netd running <n>` |
| restart | `svcctl restart vfsd` | `vfsd stopped <n>`, `vfsd running <n>` |
| baseline | `svcctl list` | both `running` |
| exit | `exit 0` | `init exited (0)` |

## In-flight / reconnect transcript

`cargo xtask qtest --case fault-reconnect --arch x86_64` (aarch64 is identical):

```
svcd: frt up
fault-reconnect: gen0=1
process: child exited (1)
fault-reconnect: inflight:5
svcd: frt up
fault-reconnect: gen1=2
fault-reconnect: stale:5
fault-reconnect: reopened
fault-reconnect: ok
process: child exited (0)
svcd: client up
svcd: settled
init exited (0)
```

The worker dies with exit code 1 *inside* the read handler, before the scheme server sends a
reply. The client's pending `READ` therefore fails with `EIO` (`5`; `EPIPE`/`32` is accepted), the
`svc:/frt.gen` generation advances from 1 to 2 on the automatic restart, the stale handle keeps
failing with `EIO` instead of hanging, and `raven_scheme::service::reopen` opens a fresh handle
that reads from the restarted worker.

`cargo xtask qtest --case fault-restart --arch x86_64` (aarch64 is identical):

```
svcd: frt up
fault-restart: gen0=1
fault-restart: read-before=ok
process: child exited (0)
svcd: frt up
fault-restart: restarted
fault-restart: gen1=2
fault-restart: stale:5
fault-restart: reopen1
process: child exited (0)
fault-restart: stopped
svcd: frt up
fault-restart: started
fault-restart: gen2=3
fault-restart: reopen2
fault-restart: ok
process: child exited (0)
svcd: client up
svcd: settled
init exited (0)
```

## Kill matrix

| Point | How | Evidence |
|---|---|---|
| idle | `svc:` STOP, then START | `stopped`, `started`, `gen2`, `reopen2` |
| after reply | a live handle from a successful read, then RESTART | `read-before=ok`, `stale:5` |
| during restart | `svc:` RESTART | `restarted`, `gen1`, `reopen1` |
| in-flight | worker exits inside the read handler | `child exited (1)`, `inflight:5`, `gen1`, `reopened` |

## Recovery and leak counts

Recovery is asserted through `svc:`: after each kill the service reports `running` again and the
generation advances. The driver records per-run wall time and the first failure in `--soak` mode.

There is no guest-visible handle/IRQ/DMA/mapping counter at this base. The handle/IRQ/DMA/mapping
drain is proven by S69's kernel-only `s69-cleanupcheck` evidence (`proc-death-cleanup`); the fault
harness asserts the restart count stays bounded, the generation advances, the process table is
stable and no panic fires.

## Limitations

* The `release` profile is x86_64-only: `profiles/release.toml` says so, `pcid` needs the ACPI
  MCFG/ECAM window, `devd` needs `pci:`, `netd` needs `nic:`, and the staged login/user programs
  are the x86_64 release build. `fault-restart --arch aarch64` therefore cannot use it; the
  cross-arch kill matrix is the self-contained `fault-restart`.
* The release image stages only `netd` and `vfsd` as resident data servers; `blockd` and
  `fsd-ufs` live in `blockd-test`/`fsd-ufs-test` and are outside S71's Owns to add to release.
* `profiles/release.d/rc.conf` pins the resident services' restart policy to `never`, so the
  release matrix restarts explicitly. The self-contained fixture uses `restart = "on-failure"`
  for the in-flight death.
* The 24 h soak mode exists but was not run in this session.

## UNVERIFIED

No Pi 4/Pi 5 board run. The harness is QEMU x86_64 and aarch64 only.
