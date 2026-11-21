<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP SMP stress cases (K284)

The release gate requires KVM `-smp 2` / `-smp 4` and TCG `-smp 4` to pass the
TCP socket path on the merged FreeBSD-derived stack. These cases boot the
`network-config-test` profile guest on SMP and run the K230 socket-inet flow
(`socket` → `bind` → `listen` → `poll` → `accept` → `recv` → `send` → `recv`
→ `close`) through QEMU user-networking hostfwd.

The cases are faithful derivations of `socket-inet`: same fixture
(`tests/fixtures/socket-inet`), same boot cmdline, same expectation set; the
only differences are the accelerator and the explicit CPU count.

| Case | Accelerator | CPUs | Extra flags |
|---|---|---|---|
| `tests/qtest/cases/tcp-smp2.toml` | KVM | 2 | `-enable-kvm -cpu host -smp 2` |
| `tests/qtest/cases/tcp-smp4.toml` | KVM | 4 | `-enable-kvm -cpu host -smp 4` |
| `tests/qtest/cases/tcp-smp4-tcg.toml` | TCG | 4 | `-smp 4` |

The appended `-smp N` overrides the harness's fixed `-smp 4` (S112 pattern:
`qemu_extra` is appended last, last wins). Each case expects the kernel's
`smp: N cpus online` line before any socket activity, then the full socket-inet
expectation sequence, and forbids `socket-inet: fail`, `panic` or `EXCEPTION`.

## Run

```text
cargo xtask qtest --case tests/qtest/cases/tcp-smp2.toml
cargo xtask qtest --case tests/qtest/cases/tcp-smp4.toml
cargo xtask qtest --case tests/qtest/cases/tcp-smp4-tcg.toml
```

KVM runs require `/dev/kvm` access (the `kvm` group) and QEMU with KVM
support; OVMF firmware is taken from `/usr/share/OVMF/OVMF_CODE_4M.fd`.
Serial evidence lands in `build/qtest/<case>.log` with the JSON result in
`build/qtest/<case>.json`.

## Results (K284 acceptance, 2026-11-21)

| Case | Result | Wall time | Exit status |
|---|---|---|---|
| `tcp-smp2` | pass | 498.30 s | 33 (guest success) |
| `tcp-smp4` | pass | 368.76 s | 33 (guest success) |
| `tcp-smp4-tcg` | pass | 510.11 s | 33 (guest success) |

All 13 expectations matched in every run; zero forbidden hits.

## Findings

* **F1: SMP console write race (pre-existing, kernel).** A design experiment
  that added `smp_user=on` (user threads spread over the APs) interleaved
  bytes of two concurrent userland console writers on the polled COM1 path:
  `debug_write` (`kernel/src/syscall/basic.rs`) reaches
  `serial::write_bytes` (`kernel/src/arch/x86_64/serial.rs`), which writes
  byte-by-byte without an SMP lock. The TCP data path itself was byte-exact in
  that run. The shipped cases therefore keep the socket-inet boot cmdline
  (user threads on the BSP by default), which keeps console output atomic and
  the gate deterministic. A kernel ticket should add the console write lock;
  after that, an `smp_user=on` TCP variant is the follow-up stress case.
* **F2: shared-host interference.** On a host running many concurrent agent
  sessions (loadavg peak ~41 on 20 cores), one acceptance run was killed by an
  external `SIGTERM` to its fixture `build.sh` during the pre step; the rerun
  at the same cap passed. Keep acceptance runs on a quiet host where
  practical.
