<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# virtio-net TX completion and ISR chain (K433)

Status: **chain traced; the 44-byte SSH-confirmation shape is deterministic and green; the
completion-drop mutation makes it fail. No red step exists in the NIC chain, so no product patch is
made.** K195's intermittent stall has **zero** `virtio-net: tx-wait` lines and netd reports
`tx=0` with `TxError::Busy` from other traffic; that locates the remaining unknown above the NIC
(netd/TCP), not in the virtio used ring or the legacy ISR. K433 therefore reports BLOCKED for the
"fix the NIC" half, with the chain and the deterministic test as evidence.

## Chain (source first)

The Raven TX path has an extra hop the BSD donors do not have: `netd` produces frames into a
shared `raven-proto-net` ring, and the driver consumes that ring and produces into the virtio
split queue.

| # | Step | FreeBSD | NetBSD | Raven |
|---|---|---|---|---|
| 1 | Enqueue a frame | `sys/dev/virtio/network/if_vtnet.c:2707` `vtnet_start_locked` → `:2502` `vtnet_txq_enqueue_buf` | `sys/dev/pci/if_vioif.c:1979` `vioif_send_common_locked` → `:73` `vioif_net_enqueue_tx`, `:89` `vioif_notify` (line 2067) | `userland/servers/netd/src/nic.rs:236` `transmit`; `:245` `reclaim_tx`; `:261` `RingProducer::push` |
| 2 | See the descriptor, build the virtio chain | `if_vtnet.c:2502` `vtnet_txq_enqueue_buf`; header + frame segments | `if_vioif.c` `vioif_net_enqueue_tx` (same functions) | `userland/drivers/virtio-net/src/main.rs:762` `pump_tx`; `:823` `RingConsumer::peek`; `:846-863` header/frame descriptors |
| 3 | Publish avail and notify queue 1 | `sys/dev/virtio/virtqueue.c:563` `virtqueue_notify` (dmamap sync, `mb()`, `vq_ring_must_notify_host`, `vq_ring_notify_host`); `if_vtnet.c:2426` `vtnet_txq_notify` → `:2433`; caller `:2747` | `if_vioif.c:2067` `vioif_notify` | `main.rs:594` `push_avail` (write slot, `avail->idx`, `publish`); `:616`/`:583` `notify_queue(notify, 1)` |
| 4 | Device completes; ISR set | `virtqueue.c:594` `virtqueue_intr_filter` (compare `used->idx`, disable intr), `:608` `virtqueue_intr`; `sys/dev/virtio/pci/virtio_pci.c:849` `vtpci_intx_intr` reads the ISR; `virtio_pci_legacy.c:717` `vtpci_legacy_read_isr` (read-to-clear) | `sys/dev/pci/virtio_pci.c:1269` `virtio_pci_intr` reads ISR at `:1275`, read acks; `sys/dev/pci/virtio.c:1508` `virtio_intrhand`; `if_vioif.c:2231` `vioif_tx_intr` → `:2187` `vioif_tx_handle_locked` → `:2074` `vioif_tx_deq_locked` | `main.rs:646` `irq.wait(10 ms)`; `:649` read ISR byte; `:651` `irq.ack`; kernel `kernel/src/driver/irq.rs:474` `wait` (unmasks legacy at `:496-498`), `:556` `ack` (re-arms at `:567-571`) |
| 5 | Consume used, free the shared slot | `virtqueue.c:722` `virtqueue_dequeue` (`vq_used_cons_idx++`) | `if_vioif.c:2074` `vioif_tx_deq_locked` → `virtio_dequeue`, `:26` | `main.rs:783` used-index compare; `:799` `tx.used++`; `:800-804` `RingConsumer::pop` (publishes `tx_cons`); `:805` `tx_busy=false`; `:807` `TX_FRAMES++` |
| 6 | Producer observes the reclaim | N/A (BSD has no split ring) | N/A | `netd/src/nic.rs:175` `reclaim_tx` reads `tx_cons`, frees the netd slot |
| 7 | Safety net | 5 s `if_timer` watchdog (`if_vtnet.c` start/intr) / NetBSD `if_vioif.c:864` `vioif_watchdog` | same | the pump runs every 10 ms regardless of `fired` (`main.rs:642-676`), so a late IRQ cannot strand a completion |

The ISR is read-to-clear on both donors and in the device model; the Raven driver reads it once on
`fired > 0` and then acks. A completion that arrives between the used check and the next wait is
still observed by the 10 ms pump, so the poll is a safety net, not the only path.

## Deterministic test (44 bytes after a read)

`tests/fixtures/virtio-tx/main.c` now finishes the 32 KiB burst with the SSH confirmation shape:

1. the host sends a 4-byte request after the echo;
2. the guest reads it and writes **four 44-byte** confirmation frames (the shared ring has only
   three spare TX buffers, so the fourth requires a completed slot to be reclaimed);
3. the host verifies all 176 bytes and only then sends `ACK\n`;
4. the serial asserts `read request=4`, `confirmation frames=4 bytes=176`, `confirmation ack`.

Measured pass (`cargo xtask qtest --case virtio-tx --arch x86_64`, 197.43 s):

```text
virtio-net: irq tx fired=1 isr=0x1 tx=1 rx=1
virtio-net: irq progress fired=24 tx=23 rx=22
virtio-tx: sent=32768 elapsed_ms=345
virtio-tx: echo received=32768
virtio-tx: read request=4
virtio-tx: confirmation frames=4 bytes=176
virtio-tx: confirmation ack
virtio-tx: done
```

Host: `received=32768`, `echoed`, `sent request=4`, `confirmation=176 frames=4 ack`. Every chain
step is therefore observed for the 44-byte shape: the driver consumed the netd ring, notified,
the device used the queue, the ISR fired (`isr=0x1`), the driver acked it, and the fourth frame
required a reclaimed slot.

## Mutation

`VIRTIO_TX_MUTATION=1` stages `tx-stall` (`tests/fixtures/virtio-tx/drivers-mutation.toml`), which
drops the TX used-ring observation. The case fails (185.87 s):

```text
virtio-net: tx-wait used=1 expected=0 avail=1 fired=0 isr=0x1
virtio-tx: echo received=-1
virtio-tx: confirmation request failed errno=110
```

The new `confirmation` assertions are unmatched and the forbidden line is hit. The mutation is
test-only and is never staged in a product profile.

## Why BLOCKED, not patched

K195 round 14 measured the live stall at `eth0 tx_packets=52 tx_errors=50`, `tcp ESTABLISHED ...
rx=0 tx=0`, and **0** `virtio-net: tx-wait` lines. `TxError::Busy` is netd's own `tx_free`
exhaustion (`nic.rs:246`), and zero `tx-wait` means the driver never held an uncompleted virtio TX
frame at that point. The frame ring stopped above the NIC. A product patch to the ISR/used-ring
path would be untested for that symptom, so this ticket stops at the traced chain and the
deterministic test, as the ticket requires ("report BLOCKED if not deterministic").

## Donor reconciliation

| Topic | Illumos (CDDL) | BSD (permissive) | Linux (GPL, read-only) | Decision |
|---|---|---|---|---|
| TX enqueue/notify | no virtio-net | FreeBSD `if_vtnet.c`/`virtqueue.c`; NetBSD `if_vioif.c` (all BSD-2-Clause) | `drivers/net/virtio_net.c` absent from the sparse checkout | Read for the chain; no code copied |
| Used-ring completion | no virtio-net | FreeBSD `virtqueue.c:722`; NetBSD `if_vioif.c:2074` | `drivers/virtio/virtio_ring.c` absent | Raven first-party; same used-index contract |
| ISR read/ack | no virtio-net | FreeBSD `virtio_pci.c:849`, `virtio_pci_legacy.c:717`; NetBSD `virtio_pci.c:1269` | `virtio_pci_legacy.c` absent | Raven kernel `irq::wait/ack` + driver ISR read |
| Watchdog / safety net | n/a | NetBSD `if_vioif.c:864` `vioif_watchdog` | absent | Raven 10 ms pump as the bounded net |

Donor revisions: FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, illumos-gate
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, NetBSD from the workspace checkout (BSD-2-Clause).
The Linux `drivers/net` and `drivers/virtio` files are absent in the sparse checkout and are
recorded rather than reconstructed.

## Files

* `tests/fixtures/virtio-tx/main.c`, `host.py` - confirmation shape and exact verification.
* `tests/qtest/cases/virtio-tx.toml` - assertion set.
* This document - chain and measurements. No product source changed.
