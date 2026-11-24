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

The `pi4-xhci` driver implements `Host` for the D1 model:

* `allocate_dma` / `free_dma` map the controller-visible DMA buffer;
* `doorbell(slot, target)` writes the xHCI doorbell;
* `wait_event(events, timeout)` inserts the next controller event into the event ring;
* `acknowledge_irq` clears the interrupter.

`XhciSeam::submit(Transfer { slot, endpoint, trbs }, timeout)` queues a Setup/Data/Status sequence,
rings the doorbell, waits for the matching Transfer Event, completes the queued TRBs and returns the
completion code and residual length. The command ring is exposed for Enable Slot, Address Device and
Configure Endpoint commands. The fixture `tests/fixtures/pi4-usb/synthetic-trb.txt` is an
own-construction TRB sequence used by the host test.

## Driver integration

`userland/drivers/pi4-xhci` has two paths:

* `pi4-xhci test` (the `pi4-usb` qtest): runs one D1 ring sequence, enumerates a synthetic boot
  keyboard through S61, binds it to the S62 `KeyboardService`, and decodes the key `a`. The qtest
  initfs contains only `raven-init` and the driver.
* default: maps BAR0, the function configuration page, a DMA buffer and the legacy IRQ through
  `devd`, then drives the standard xHCI controller through `raven-xhci` and the S61/S62 path.

The qtest serial:

```
pi4-xhci: ring trbs=3 completion=success
pi4-xhci: device vendor=0x0627 product=0x0001 class=0 configs=1 interfaces=1
pi4-xhci: hid interface=0 report=63 ep=0x81
pi4-xhci: keyboard boot protocol ok
pi4-xhci: key=a
pi4-xhci: ok
```


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

## Pi 4 release staging (USB1)

The Pi 4 release profile now stages the pieces that were missing:

| Item | Path | Role |
|---|---|---|
| xHCI probe | `/sbin/pi4-xhci` | PCIe VL805 enumeration and S61/S62 boot-keyboard decode |
| keyboard queue | `/sbin/usb-hid`, service `usb-hid` | serves `kbd:` over the bounded `ConsoleInput` queue |
| boot probe | service `usb-xhci` | runs `devd test` once after `pcid` and `usb-hid`; never registers `dev` |
| driver entry | `profiles/pi4-release.d/drivers.conf` | PCI ID `1106:3483`, BAR0, DMA and legacy IRQ |

`usb-xhci` is `required = false` and `restart = "never"`: a missing VL805, keyboard, BAR or IRQ
leaves UART getty/login untouched. `usb-hid` is also optional; with no keyboard its `kbd:` queue is
empty and `READ` returns `EAGAIN`. The platform `devd dtb launch` still owns `dev:` and the GENET/
platd entries; `devd test` does not register `dev`.

## PCIe, VL805 BAR and IRQ

The Pi 4 BCM2711 exposes a PCIe root complex. The USB 3 host function is the VIA VL805
(`vendor 0x1106`, `device 0x3483`, xHCI class `0x0c0330`) on that bus. The board revision may
report a different revision/ID; the value is read from config space, not assumed by the driver.

| Resource | How it is obtained | Handoff |
|---|---|---|
| BAR0 | PCI config BAR0; size comes from `devd` as `bar_size=` | `devd` opens `pci:/<bdf>/bar0` and transfers it; child handle 3 |
| Config space | `pci:/<bdf>/config` page | transferred as child handle 4 |
| DMA | `devd` allocates the requested length and appends `dma=<len>` | child handle 5; `pi4-xhci` checks `MIN_DMA_LEN` |
| IRQ | legacy line from PCI config, `resources=irq` | child handle 6; `pi4-xhci` waits/acks through `IRQ_HANDLE` |
| Doorbells/rings | BAR0 offsets; command/event/transfer rings in DMA | S61 `RealSeam`/`Host` |

`pi4-xhci` also reads the configuration page for the vendor/device IDs before touching BAR0. The
VL805 board BAR size, IRQ routing through the GIC, and the actual `pci:` BDF are **UNVERIFIED**;
UI1 records them. The model and qtest use the D1 synthetic seam, not a hardware claim.

## HID report and console mapping

The S60 decoder consumes the standard 8-byte boot-keyboard report:

| Report byte | Meaning |
|---|---|
| 0 | modifier bitmap (Ctrl, Shift, Alt, GUI) |
| 1 | reserved |
| 2..=7 | up to six HID usages; `0` is empty |

The keyboard service selects the boot protocol, validates the report descriptor and queues ASCII
on a press only (releases never queue). US layout is the first layout; the model also carries the
UK map. The requested set is:

| Input | Console byte |
|---|---|
| printable US key | its ASCII byte |
| Shift + letter | uppercase |
| Enter | `0x0a` |
| Backspace | `0x08` |
| Ctrl-D | `0x04` (EOT) |
| Ctrl/Shift/Alt alone | no byte |

The queue is `usb_hid::ConsoleInput`, 256 bytes, single-reader. `usb-hid` exposes it as `kbd:`:
`READ` drains bytes (empty → `EAGAIN`), `WRITE` appends (full → `ENOSPC`). The xHCI driver pushes
the decoded bytes to `kbd:` best-effort; the fake `pi4-usb` qtest drains its local queue. CON1 will
consume `kbd:` for the HDMI console; UART remains the recovery path.

A non-keyboard HID descriptor is rejected by `KeyboardService::init`; a mouse interface is not
bound to the keyboard service. There is no hotplug/re-enumeration beyond the boot-time probe in
USB1; hotplug remains the S66 follow-up.

## Failure modes

| Failure | Effect | Boot impact |
|---|---|---|
| No VL805/PCIe function | `devd test` reports no matching driver and exits 0 | none; UART getty/login works |
| BAR/config/DMA/IRQ mapping fails | `pi4-xhci` prints the error; `usb-xhci` fails optionally | none; `required = false` |
| No keyboard attached | `usb-hid` serves an empty `kbd:` queue; xHCI probe finds no device | none |
| Non-keyboard HID | descriptor/class rejected cleanly; no bytes queued | none |
| Endpoint stall/reset | driver reports the error and exits; queue unchanged | none |
| `usb-hid` absent/crashed | xHCI push is best-effort and drops the bytes; local transcript remains | none |
| Queue full | `ENOSPC`; newer bytes are not silently overwritten | none |
| UART failure | separate `consd`/`DEBUG_READ` path; USB does not replace it | independent |

The release image still boots with no keyboard. A keyboard is never a boot dependency; only UI1's
interactive HDMI login requires one.

## Board evidence plan (UI1)

1. Boot the Pi 4 release image from SD (SD1) with UART attached; capture the full serial log.
2. Confirm `pcid` sees the VL805 function and `usb-xhci` logs the PCI match/BAR/config/DMA/IRQ
   handoff. If `pcid` cannot reach the PCIe bus, record the exact failure; do not fake a match.
3. Attach a boot-protocol USB keyboard before boot; confirm the `pi4-xhci` device/HID/key lines and
   `usb-hid: kbd queue ready`.
4. On HDMI, type the root password and `uname`; the bytes must appear through `kbd:`. Type
   Enter, Backspace and Ctrl-D and record the console bytes.
5. Repeat with the keyboard unplugged: UART getty/login must still work; HDMI shows the login
   prompt with no keyboard bytes and boot does not wait.
6. Record the board revision, VL805 ID, BAR size, IRQ line, serial/HDMI transcript paths and any
   divergence. Keep the first failure; do not replace it with a synthetic result.

## DR-57 / DR-52 donor reconciliation

Revisions from `ravenbsd-resources/donors.lock`: `netbsd 36eddebcc36cb0775aa2962005b9c05419a6ff01`,
`freebsd 133344405f532bd54ff3629a77d4c4e062f01fdb`, `illumos-gate
586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, `linux adc218676eef25575469234709c2d87185ca223a`.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| xHCI controller/rings | `illumos-gate/usr/src/uts/common/io/usb/hcd/xhci/xhci.c` (CDDL-1.0): first BAR, command/event rings, interrupt/event handling; design/quarry only | NetBSD `sys/dev/usb/xhci.c` (BSD-2-Clause) and `xhcireg.h`: command/event/transfer ring and port state machine | `linux/drivers/usb/host/xhci.c` (GPL-2.0) read-only outcome | First-party Rust `raven-xhci`/`raven-pi4-xhci`; no CDDL/GPL copy; the BSD driver is kernel-coupled, so behaviour was read and the userspace model/register constants were independently written |
| Boot keyboard/HID | `illumos-gate/usr/src/uts/common/io/usb/clients/hid/hid.c` (CDDL-1.0): HID parser/client split | NetBSD `sys/dev/usb/ukbd.c` (BSD-2-Clause): boot report, modifier/keycode translation, press/release | Linux `drivers/hid/usbhid` (GPL-2.0) read-only | First-party `raven-usb-hid`/`usb-hid`; the mapping and queue are original Rust; no donor code copied |
| PCI/VL805 handoff | No Pi-specific VL805 path; PCIe/PCI resource model is the idea | NetBSD PCI resource allocator (BSD) and FreeBSD `xhci_pci` (permissive) as behaviour references | Linux `xhci-pci.c` read-only | `pcid` + `devd test` + handle transfer; no code copied |
| Console seam | Doors/STREAMS are the service multiplexing quarry; no keyboard driver in the base | BSD `tty`/`wskbd` input-to-console layering | Linux input subsystem outcome only | Bounded `ConsoleInput` + `kbd:` scheme; CON1 consumes it; no shared-memory/process-server shortcut |
| Tests | N/A | N/A | N/A | Host unit tests and the synthetic `pi4-usb` qtest; board UNVERIFIED |

No `PROVENANCE.toml` import is added: no permissive donor file was transpiled or copied. The
existing first-party crates and the D1/D2 model predate this staging ticket.
