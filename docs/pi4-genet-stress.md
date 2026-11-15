<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 GENET ring stress and fault matrix (O5)

Status: host-only deterministic stress. No Pi 4 board run was performed for O5; every real-link
result remains UNVERIFIED. The tests live in `tests/genet` and drive the public `raven-genet`
state machines; they touch no MMIO, DMA or hardware.

## Fault matrix

| Fault / stress | Test | Iterations / shape | Result |
|---|---|---|---|
| Full RX ring wrap | `full_rx_ring_wraps_256_slots_repeatedly` | 4 × 256 posts/completions | PASS |
| Full TX ring wrap | `full_tx_ring_wraps_256_slots_repeatedly` | 4 × 256 queue/reclaims | PASS |
| Still-owned RX and TX | `still_owned_descriptors_do_not_advance_and_recover` | one held slot each, then completion | PASS |
| Malformed RX mix | `malformed_rx_statuses_are_counted_without_stalling` | 256 statuses, 128 errors | PASS |
| Malformed TX completions | `malformed_tx_completions_free_the_ring` | 256 statuses, 128 errors | PASS |
| Bounce boundaries | `bounce_boundaries_hold_over_long_address_sweeps` | 1,024 sweep points + 1,024 direct points | PASS |
| Link flap under traffic | `link_flaps_under_continuous_traffic` | 64 traffic rounds, 32 link transitions | PASS |
| IRQ coalescing | `irq_coalescing_reclaims_every_descriptor` | 256 RX + 256 TX, budget 1 then 7 | PASS |
| Interrupt cause decode | `interrupt_status_coalesces_rx_and_tx_causes` | RX+TX combined status | PASS |
| Recovery budget | `recovery_budget_is_bounded_under_repeated_faults` | 3 resets × 8 errors, then fault | PASS |

## What each test proves

- **Ring wrap**: a full 256-slot ring advances producer/consumer through four complete wraps with
  no lost slot and no index drift.
- **Ownership**: a descriptor whose device `OWN` bit is still set is not completed or advanced;
  the same slot completes after the bit clears.
- **Malformed input**: CRC, short-frame and zero-length RX statuses, and zero/oversized TX
  completions, are counted while the slot is freed and the ring continues.
- **Bounce limits**: addresses at the 16 GiB window edge route directly up to the boundary and
  return `BounceRequired` across it; a direct caller-supplied bounce buffer is accepted; the
  low-address sweep stays direct; oversized requests return `TooLong`.
- **Link flap**: RX/TX post/complete traffic continues while the debounced link tracker emits
  Up/Down events across alternating samples.
- **IRQ coalescing**: one-descriptor budgets truncate and report the outstanding work; subsequent
  seven-descriptor budgets drain all 256 descriptors in both directions.
- **Recovery**: the default policy grants three ring resets then returns `Fault`; a success clears
  the consecutive-error counter.

## Running

```
cargo test -p raven-genet-stress
cargo test -p raven-genet
cargo test -p genet
make check-fast
```

`tests/genet` is a host-testable workspace member included in `default-members`, so the normal
`cargo test`/`make check` host suite also runs this matrix.

## Gaps

- The matrix is deterministic; it is not a fuzzer and uses no threads, randomness or timing.
- It exercises the model, not the `MmioRegion`/`DmaRegion` board seam or a real GENET engine.
- No QEMU model exists for GENET; the Pi 4 log remains the only real-link gate.
- Link speed, filter acceptance and interrupt masking are represented by model values, not a
  board measurement.

## Provenance

No donor source was read for O5; the tests use the clean-room `raven-genet` model built in
O1-O4. The donor-file list for that model is in `docs/pi4-genet.md`. `PROVENANCE.toml` is
unchanged.
