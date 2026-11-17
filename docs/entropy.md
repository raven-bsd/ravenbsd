<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Entropy

Status: PROPOSED (S72). Implements the pure pool in `crates/raven-entropy` and the kernel seed
path + `GETRANDOM`; the Pi 4 RNG200 path is documented but not wired (no RNG200 driver yet).

## Source, fallback and reseed rule

* **Source (in order)**: RDRAND (x86-64, gated on `CPUID.1:ECX[30]`), then RNDR (aarch64, gated
  on `ID_AA64ISAR0_EL1[63:60] == 1`), then the Raspberry Pi 4 RNG200 (MMIO `0x7e104000`; not
  wired in this ticket). Each provides 64 bytes of seed material.
* **Fallback**: timer jitter — the low 16 bits of the monotonic-clock delta between tightly spaced
  reads. This is a **weak, possibly observable** source; it exists only so boot and early
  userspace never block, and it is never claimed to be cryptographically strong.
* **Reseed rule**: `raven-entropy` re-keys whenever a full 64-byte accumulator of material has
  been supplied, and refuses to emit more than 64 KiB since the last re-key
  (`Error::NeedsReseed`), at which point the kernel folds in fresh jitter and retries. An
  unseeded pool returns `Error::Unseeded`.

On QEMU the default CPUs (`qemu64`/`cortex-a72`) expose neither RDRAND nor RNDR, so the qtest
exercises the jitter fallback; the hardware paths are compiled and the RDRAND logic mirrors the
existing `random16` path.

## Construction

`raven-entropy` is `no_std`, allocation-free, `#![forbid(unsafe_code)]`, and compiled into the
kernel from the same source via `#[path]`. It uses only the ChaCha20 block function (RFC 8439,
verified against the known-answer test): the accumulator is folded into a fresh key with one
ChaCha20 block, and output is the ChaCha20 keystream under that key. No cipher is invented; the
only primitive is the admitted ChaCha20 construction.

## Threat model

Output is exactly as strong as the accumulated seed. Seeded from a hardware DRNG (RDRAND/RNDR/
RNG200) it inherits that device's strength; seeded from jitter alone it is **not** cryptographically
secure. The pool never blocks and never claims more than its seed provides. `GETRANDOM` returns
`EAGAIN` only when no source at all can be sampled (the jitter fallback normally prevents this).

## Syscall

* `GETRANDOM` (`0x08`, reviewer-assigned; not yet a `raven-abi` variant):
  `getrandom(buf, len, flags) -> n`, capped at `0x1FFF_FFFF`. `GRND_NONBLOCK`/`GRND_RANDOM` are
  accepted and ignored (the kernel never blocks on entropy). `AT_RANDOM` seeding stays on the
  existing `random16` path until `kernel/src/process/mod.rs` is in this ticket's Owns (see the
  report).
