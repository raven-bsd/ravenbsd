<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Guest xHCI driver (S67)

Status: host-tested controller plus a QEMU `qemu-xhci` + `usb-kbd` enumeration. **No Pi 5 board
run and no keyboard key injection yet.** The QEMU transcript proves the guest xHCI register/ring
path and the S61→S62 binding; S63 owns key injection and the console/TTY transcript. The Pi/RP1
DesignWare register offset remains UNVERIFIED (R4).

## 1. What changed

* **`crates/raven-xhci`**: a guest xHCI controller over a `Seam` MMIO/DMA boundary.
  * capability decode (`CAPLENGTH`, `HCIVERSION`, `HCSPARAMS1`, `HCCPARAMS1`, CSZ);
  * controller reset/run, `DCBAAP`/`CRCR`/`ERSTSZ`/`ERSTBA`/`ERDP`, interrupter `IMAN`;
  * command/event/transfer producer rings in one DMA buffer, cycle bits and Link TRBs;
  * port power/connect/reset and the speed ID from `PORTSC`;
  * Enable Slot, Address Device, Configure Endpoint;
  * the xHCI Setup/Data/Status control sequence and interrupt-IN transfers.
* **`userland/drivers/xhci`**: the userspace driver launched by `devd`. It maps BAR0, the
  configuration page, the 64 KiB DMA buffer and the legacy IRQ, then uses `raven-usb-core`'s
  `Enumerator` and `usb-hid`'s `KeyboardService`.
* **`tests/fixtures/usb-keyboard/`**: a one-shot initfs with `raven-init`, `pcid`, `devd`, the
  xHCI driver, the PCI/devd manifests, `drivers.toml` and `svcd.conf`.
* **`tests/qtest/cases/usb-keyboard.toml`**: QEMU `-device qemu-xhci -device usb-kbd`.

## 2. Controller layout

The driver receives one 64 KiB DMA buffer and uses fixed, aligned regions:

| Offset | Region |
|---|---|
| `0x0000` | device-context base address array |
| `0x0800` | slot-1 device context |
| `0x1000` | input context (slot + EP0/EP1) |
| `0x2000` | command ring (63 usable + Link) |
| `0x3000` | event ring (64 TRBs) |
| `0x3400` | event ring segment table (one entry) |
| `0x4000` | EP0 transfer ring |
| `0x5000` | EP1 transfer ring |
| `0x6000` | control-transfer buffer |
| `0x6100` | interrupt-IN report buffer |

The input-context encoder handles the 32- or 64-byte context size from `HCCPARAMS1.CSZ`. Address
Device uses slot/EP0 (DCI 1); Configure Endpoint adds the interrupt IN endpoint (DCI 3) with the
descriptor's max packet and interval.

## 3. Enumeration path

```
Controller::new          read capability parameters
Controller::init         reset/run, rings, ports
wait_for_port            PORTSC.CCS
reset_port               PORTSC.PR/PRC/PED
enable_slot              Enable Slot command
address_device           Address Device, EP0 ring, initial max packet 8
S61 Enumerator           device/config descriptors, address pool, SET_CONFIGURATION
Configure Endpoint       interrupt IN endpoint from the config descriptor
GET_DESCRIPTOR(0x22)     HID report descriptor (interface recipient)
KeyboardService::init    SET_PROTOCOL(boot), SET_IDLE, parse the descriptor, prime EP1
```

`ControlAdapter` treats `SET_ADDRESS` as the already-completed Address Device step (xHCI assigns
the address through the command), while all other standard/class control transfers use the EP0
ring. `HidTransport` implements the S60/S62 `TransferRing` seam over the same controller.

## 4. QEMU evidence (OBSERVED)

```
$ cargo xtask qtest --case usb-keyboard --arch x86_64
usb-keyboard: pass
exit 0
```

Serial excerpt (`build/qtest/usb-keyboard.log`):

```
pcid: device 00:03.0 vendor=0x1b36 device=0x000d class=0x0c subclass=0x03 irq_line=11 irq_pin=1
pcid: devices=7
svcd: pcid up
devd: matched 00:03.0 vendor=0x1b36 device=0x000d irq_line=11 irq_pin=1
devd: driver xhci bar=0 size=16384
devd: driver xhci dma bus=0x719000 len=65536
devd: driver xhci irq line=11
xhci: bar=0 size=16384 dma=65536 bus=0x719000 vendor=0x1b36 device=0x000d
xhci: version=0x0100 slots=64 ports=8 csz=0
xhci: port=5 speed=3
xhci: slot=1 addressed
xhci: device vendor=0x0627 product=0x0001 class=0 configs=1 interfaces=1
xhci: hid interface=0 report=63 ep=0x81
usb-keyboard: boot protocol ok
usb-keyboard: enumerated keys=0
usb-keyboard: ok
devd: driver xhci cycle 1 exit=0
svcd: settled
init exited (0)
```

`build/qtest/usb-keyboard.json` reports `pass`, 18 matched expectations and no forbidden hits.

## 5. S63 follow-up

S67 does not inject keys. QEMU `usb-kbd` waits for a host `sendkey`/QMP event; the current qtest
harness has no monitor/QMP step. S63 needs one of:

* a QEMU monitor/QMP key source in the harness (preferred), or
* a test-only USB keyboard device with a scripted report sequence.

Once a key report arrives, `HidTransport::next_report` returns the eight bytes and S62's
`KeyboardService` queues the pressed-key ASCII. S63 then asserts the console/TTY bytes.

## 6. Board boundary

The Pi 5 path is not claimed:

1. The RP1 USB nodes are DesignWare `snps,dwc3`; the xHCI register offset inside the DWC3 window
   and the DWC3 core setup are UNVERIFIED (R4).
2. The R1 PCIe/BAR/MSI path must be confirmed on the board.
3. The Pi firmware/EEPROM and RP1 firmware load are separate (B55/R1).
4. No Pi USB device has been enumerated; QEMU proves the standard xHCI sequence only.

## 7. Tests

`cargo test -p raven-xhci` covers:

* context size (CSZ 32/64), Address Device slot/EP0 encoding, Configure Endpoint EP1 encoding and
  invalid sizes/buffers;
* producer Link/cycle wrap, event-ring FIFO/cycle and ring-size validation;
* QEMU capability decode, port connect/speed and the no-port timeout;
* the QEMU xHCI vendor/device constants and DMA layout bounds.

The driver is compiled and linted for `x86_64-unknown-none`; the QEMU case is the integration gate.

## 8. Provenance

The xHCI register/TRB/context shapes are the public xHCI 1.1 specification. The guest model reuses
R4's `raven-rp1-xhci` register/TRB constants and the S61/S62 project APIs. No Linux, BSD, GPL or
vendor xHCI driver code was copied, translated or closely paraphrased. QEMU is the test emulator,
not a donor implementation. The RP1 firmware and any real USB device data remain outside this
repository.
