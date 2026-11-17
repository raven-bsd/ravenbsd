<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# S71 fault-injection harness for server death

Status: S71 baseline. The harness boots the release profile, logs in, and restarts the resident
data servers through `svcctl`, then asserts the kernel and shell survive, the services return to
`running`, and the restart counts are stable. The S70 generation/reconnect scenario is not present
on this base.

## Driver

```sh
sh scripts/fault-injection.sh --profile release --arch x86_64   # one baseline run
sh scripts/fault-injection.sh --soak 86400                      # 24 h soak (repeat the baseline)
cargo xtask qtest --case fault-restart --arch x86_64            # the qtest the driver runs
```

`fault-restart` (`tests/qtest/cases/fault-restart.toml`) boots `profile = "release"`, logs in as
root, and drives:

| Step | Command | Expected |
|---|---|---|
| baseline | `svcctl list` | `netd running <n>`, `vfsd running <n>` |
| restart | `svcctl restart netd` | `netd stopped <n>`, `netd running <n>` |
| stop | `svcctl stop netd` | `netd stopped <n>` |
| start | `svcctl start netd` | `netd running <n>` |
| restart | `svcctl restart vfsd` | `vfsd stopped <n>`, `vfsd running <n>` |
| baseline | `svcctl list` | both `running` |
| exit | `exit 0` | `init exited (0)` |

A `panic`, an `EXCEPTION`, a `command not found`, or a failed login fails the run.

## Kill matrix (baseline)

The deterministic points exercised are **idle** (restart after boot) and **during restart** (the
stop/start transition). The **in-flight request**, **after reply** and **client reconnect with a
generation change** points need the S70 service-generation work and are the missing scenario.

## Recovery and leak counts

Recovery is verified through `svcctl status`: after each kill the service prints `running` again.
The per-run wall time is recorded by `fault-injection.sh --soak`; the first failure timestamp is
reported. There is no guest-visible handle/IRQ/DMA/mapping counter at this base: the leak proof
lives in the kernel-only `s69-cleanupcheck` evidence (S69 `proc-death-cleanup`). The harness
therefore asserts the restart count stays bounded and the process table is stable, and relies on
S69 for the handle/IRQ/DMA/mapping drain.

## Limitations

* The release image stages only `netd` and `vfsd` as resident data servers; `blockd` and
  `fsd-ufs` are in their own test profiles (`blockd-test`, `fsd-ufs-test`) and are outside S71's
  Owns to add to the release image.
* `profiles/release.d/rc.conf` pins the resident services' restart policy to `never`, so there is
  no automatic on-failure restart in the release image; the harness restarts explicitly.
* The in-flight/after-reply client-error, generation change and bounded reopen are S70; until it
  merges, S71 is PARTIAL with this baseline.

## UNVERIFIED

No Pi 4/Pi 5 board run. The harness is QEMU x86_64 only.
