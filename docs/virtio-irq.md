<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# virtio-pci interrupts on the block path (K427)

Status: **the `disk-tools` gate is green with a bounded used-ring poll.** The modern virtio-pci
device keeps working with legacy INTx disabled, but the platform currently cannot give the block
and network functions independent interrupt lines. This document records the measured queue, ISR
and used-ring state, why the shared legacy line failed, and the exact bound of the replacement
poll. The actual root cause (one exclusive legacy GSI for two functions, and MSI-X setup failing)
is outside the block driver and stays open for `devd`/`kernel` (see *Owner note*).

## Symptom

`tests/qtest/cases/disk-tools.toml` attaches one `virtio-blk-pci` and one `virtio-net-pci` with
`disable-legacy=on`. On `origin/master` `620513c5` the case failed after 366 s:

```text
virtio-blk: dma bus=0x100000 len=65536 irq=11
virtio-blk: registered disk
devd: driver virtio-net msix unavailable: bad handle or file descriptor; using legacy irq
devd: driver virtio-net irq line=11
devd: driver virtio-net launch failed: irq: resource busy
svcd: devd failed: exited before it became ready
blockd: waiting for disk:0: no such file or ...
svcd: blockd failed: readiness timeout
```

The block driver claimed the legacy line first. The network function fell back to the same line,
its exclusive `irq_create` returned `EBUSY`, `devd` exited, and no process ever served `disk:0`.
The block queue itself was never the failing operation: no request was issued, because `blockd`
never started.

## Interrupt model and trace

`devd` prefers one MSI-X vector per function and falls back to the PCI interrupt line. In the
`disk-tools` profile MSI-X setup fails for both functions (`bad handle or file descriptor`), so
both use line 11. `devd` opens the line for each function; the second open is `EBUSY`.

| Observation | Pre-fix `disk-tools` | Post-fix `disk-tools` |
|---|---|---|
| Block queue | 0, size 8 | 0, size 8 |
| Block interrupt | legacy line 11 | legacy line 11 released before the net is launched |
| Block MSI-X | unavailable | unavailable |
| Net MSI-X | unavailable | unavailable |
| Net interrupt | line 11, `EBUSY` | line 11, claimed |
| Net TX IRQ | never | `irq tx fired=1`, `fired=24 tx=2 rx=2` |
| Block completion | request never issued | `virtio-blk: completion polled=2` |
| Disk service | `disk:0` absent | `blockd: remote disk=disk:0 sectors=32835` |

The working network queue (K396) proved that event delivery is not broken in general: the network
ring fires, acks the ISR and drains 24 frames. The difference is device/line ownership, not the
virtio ring format.

## Handling

`userland/drivers/virtio-blk/src/main.rs` no longer waits on an interrupt:

* It closes `IRQ_HANDLE` (handle 6) after mapping the BAR, config and DMA windows and before the
  request scheme starts, so `devd` can hand line 11 to the network function. The close error is
  ignored because an MSI-X profile may have no legacy line.
* A request publishes the avail ring, notifies the queue and then polls `used->idx` with a
  **bound of `POLL_LIMIT = 2000` iterations of a 1 ms sleep: 2 s**, the same budget the previous
  interrupt wait allowed.
* The first completion prints `virtio-blk: completion polled=N` once, where `N` is the number of
  1 ms polls that preceded the used-ring advance. This is the measured worst case for the profile.
* If `used->idx` advances by more than one entry or the bound expires, the driver takes its
  existing `fail()` path (reset, `EIO`), so a lost completion cannot hang the server.

Measured used-ring progress (`build/qtest/disk-tools.log`, post-fix runs):

```text
virtio-blk: completion polled=2      # first green run
virtio-blk: completion polled=1      # restore run
blockd: gpt ok partitions=1
init exited (0)
```

In the `ufs-demo` profile MSI-X succeeds and the device is not shared with a second legacy line;
the same code prints `virtio-blk: msix table=0x0 bar=1 vector=1` and
`virtio-blk: completion polled=0`, so the poll adds no measurable latency when the used entry is
already visible.

## Negative check

The used-ring read was temporarily replaced by a constant (`let used = 0u16;`), so a completion
could never be observed. `disk-tools` then failed after 356 s:

```text
blockd: remote disk=disk:0 sectors=32835 ro=0
blockd: gpt prefix: input/output error
svcd: blockd failed: exited before it became ready
```

The mutation commit was dropped and `disk-tools` passed again (247.73 s and 254.50 s across the
two green runs). The poll bound and the used-ring check are therefore load-bearing, not dead code.

## Owner note / open gap

The block driver workaround is necessary but not the product fix. The platform has:

1. one exclusive legacy GSI per function (`raven_rt::irq_create` with `IRQ_LEGACY`), so two
   functions cannot share the PCI interrupt line; and
2. MSI-X table programming that returns `bad handle or file descriptor` in this QEMU guest,
   forcing the legacy fallback.

Both live in `devd` and the kernel IRQ layer, not in `userland/drivers/virtio-blk/**`. K413
recorded the same ownership. When the platform can allocate a distinct MSI-X vector per function
(and the table open succeeds), the poll can be replaced by the normal interrupt wait without
changing the ring code. Until then the poll is the documented, bounded divergence.

## Donor reconciliation

| Topic | Illumos (CDDL, read-only) | BSD (permissive, ported) | Linux (GPL, read-only outcome) | Decision |
|---|---|---|---|---|
| Used-ring completion | `usr/src/uts/common/io/virtio/virtio_main.c` interrupt setup and `virtio_intr_pri`; no separate block poll | `sys/dev/virtio/virtqueue.c` `virtqueue_intr_filter` (594), `virtqueue_intr` (608), used-index consumption in `virtqueue_dequeue` (731) | `drivers/virtio/virtio_ring.c` used-index/event suppression outcome | First-party poll over the same `used->idx` rule; the FreeBSD queue contract is the behavioural source |
| ISR acknowledgement | `virtio_main.c` `virtio_interrupts_setup`/teardown | `sys/dev/virtio/pci/virtio_pci_legacy.c` `vtpci_legacy_read_isr` (717); `virtio_pci_modern.c` `vtpci_modern_read_isr_1` (1384) | `virtio_pci_legacy.c` ISR read | The driver no longer consumes the ISR because it does not own a line; `devd`/kernel own the line |
| Interrupt allocation | `virtio_main.c` `virtio_interrupts_setup` (MSI-X vs INTx) | `sys/dev/virtio/pci/virtio_pci.c` `vtpci_alloc_intr_intx`/`vtpci_intx_intr` | same outcome | Out of this driver's Owns; recorded as the owner gap |
| Shared line failure | one interrupt per function | `vtpci_alloc_intr_*` exclusive allocation | platform IRQ descriptor is exclusive | Driver releases its handle; product fix belongs to `devd`/`kernel` |

Donor revisions are from `ravenbsd-resources/donors.lock` (`freebsd`
`133344405f532bd54ff3629a77d4c4e062f01fdb`). Illumos is CDDL and was read only; no code was
copied or translated. FreeBSD files are BSD-2-Clause and were used as the behavioural contract,
not copied verbatim.
