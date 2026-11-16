<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 xHCI/USB host model

`crates/raven-pi4-xhci` is the host-testable model for the Pi 4 USB 3 host function. The
firmware exposes the USB 3 controller as a VL805 xHCI function behind the BCM2711 PCIe bridge; the
controller register layout is the standard xHCI 1.1 map, while the VL805/PCIe BAR, DMA and
interrupt wiring remain board integration work.

## Model

| Module | Contents |
|---|---|
| `regs` | capability/operational/runtime/doorbell offsets, `USBCMD`/`USBSTS`, `PORTSC`, `Capability`, `PortStatus`, VL805 PCI IDs |
| `trb` | 16-byte TRBs, type/completion codes, Setup/Data/Status/Link/command/event constructors |
| `ring` | command, transfer and event rings with cycle/wrap rules and Link TRBs |
| `port` | port state machine and roothub change bits |
| `seam` | `Host` DMA/IRQ/doorbell hooks and `XhciSeam::submit` for the S61 USB core |

The command and transfer rings reserve the last slot for a Link TRB and flip the producer cycle on
wrap. The event ring is FIFO and reports `Full` rather than overwriting an unconsumed event.

## S61 seam

S61 `raven-usb-core` implements `Host`:

* `allocate_dma` / `free_dma` map the controller-visible DMA buffer;
* `doorbell(slot, target)` writes the xHCI doorbell;
* `wait_event(events, timeout)` inserts the next controller event into the event ring;
* `acknowledge_irq` clears the interrupter.

`XhciSeam::submit(Transfer { slot, endpoint, trbs }, timeout)` queues a Setup/Data/Status sequence,
rings the doorbell, waits for the matching Transfer Event, completes the queued TRBs and returns the
completion code and residual length. The command ring is exposed for Enable Slot, Address Device and
Configure Endpoint commands. The fixture `tests/fixtures/pi4-usb/synthetic-trb.txt` is an
own-construction TRB sequence used by the host test.

## Board steps (UNVERIFIED)

1. Locate the VL805 xHCI function on the BCM2711 PCIe bus and map its BAR.
2. Allocate command/event/transfer rings and the device context array in DMA memory, program
   `CRCR`, `DCBAAP`, `ERSTBA`/`ERSTSZ` and `CONFIG`, then set `USBCMD_RUN`.
3. Route the xHCI interrupt through the Pi 4 GIC and program `IMAN`/`IMOD`/`ERDP`.
4. Connect a USB device, reset the port, read `PORTSC` and run Enable Slot / Address Device /
   Configure Endpoint.
5. Enumerate a HID keyboard through S61 and read its boot reports through S60.

No board run exists; every hardware result above is UNVERIFIED. The model proves only the register,
TRB, ring, port and seam logic on the host.
