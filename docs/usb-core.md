<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# USB core enumeration over xHCI (S61)

Status: host-side model only. **No USB device or Pi 5 board has been run.** The descriptor shapes,
setup packets and standard requests are the public USB 2.0 definitions; the xHCI transfer/event
rings come from R4's model. The real controller register path, DMA, port routing and every
enumerated-device result are UNVERIFIED until a board log exists. HID parsing and reports are S60's
work and are deliberately not implemented here.

## 1. What the crate owns

`crates/raven-usb-core` is `no_std` outside tests, `#![forbid(unsafe_code)]`, and depends only on
`raven-rp1-xhci` (the R4 model).

| Module | Responsibility |
|---|---|
| `descriptor` | Device (18 B), configuration header (9 B), interface (9 B) and endpoint (7 B) descriptors; full configuration-set parsing with interface alt settings, per-interface endpoint counts, duplicate checks, class-specific records preserved as raw bytes. |
| `address` | Deterministic USB address pool: address 0 is the default, assigned addresses are 1..=127, lowest free first, explicit reserve/release. |
| `control` | `SetupPacket` encode/decode, standard request codes, descriptor types, `GET_DESCRIPTOR`/`SET_ADDRESS`/`GET`/`SET_CONFIGURATION`/`SET_INTERFACE` constructors and the `ControlTransport` seam. |
| `enumerate` | `Attached -> Described -> Addressed -> Configured` enumeration with address release on a failed configuration. |
| `hub` | Attach/power/reset/enable/suspend/resume/detach/over-current port state and change bits. |
| `seam` | Setup/Data/Status TRB sequence over `raven_rp1_xhci`'s `TransferRing`/`EventRing`, chain/endpoint/slot stamping and typed transfer, command and port events. |

## 2. Descriptor parsing

The parser checks the declared length and type of every standard descriptor, `bMaxPacketSize0`
(8/16/32/64), `wTotalLength`, a non-zero configuration value, `bNumInterfaces` against the unique
interface numbers, each interface's `bNumEndpoints` against the parsed endpoints, duplicate
interface number/alternate pairs and duplicate endpoint addresses. An endpoint before its
interface, a nested configuration and a class-specific descriptor before its interface are
rejected.

Endpoint addresses are decoded to number/direction and a device context index
(`2 * number + IN`). Transfer type, max packet size and interval are exposed. Class-specific
records (for example the S60 HID descriptor) are kept as `Vec<Vec<u8>>` on the interface; the core
does not interpret them.

The fixture `tests/fixtures/usb-core/synthetic-device.txt` is a synthetic vendor-specific device
(one interface, one interrupt IN endpoint). It is not a recorded device and contains no HID data.

## 3. Enumeration sequence

```
GET_DESCRIPTOR(Device, 0, 18)
    -> parse DeviceDescriptor
assign address (1..=127)
SET_ADDRESS(address)
GET_DESCRIPTOR(Configuration, 0, 9)
    -> ConfigurationHeader, wTotalLength
GET_DESCRIPTOR(Configuration, 0, wTotalLength)
    -> parse the full set
SET_CONFIGURATION(bConfigurationValue)
    -> Configured
```

A failure after `SET_ADDRESS` releases the assigned address back to the pool. `SET_INTERFACE` is
available once configured and validates that the interface/alternate exists. String descriptors,
HID reports and class requests are intentionally outside S61.

## 4. xHCI transfer-ring seam

`XhciSeam` wraps R4's `TransferRing` and `EventRing`:

* `queue(&ControlTransfer)` turns the Setup/Data/Status sequence into 2 or 3 TRBs, sets the chain
  bit on all but the final stage, stamps the endpoint DCI and slot ID and checks free slots before
  pushing (a full ring never receives a partial sequence);
* `push_event(Trb)` publishes a controller event;
* `poll()` decodes Transfer Event, Command Completion, Port Status Change and other events;
* `complete_transfer()` advances the host-side ring model.

The DWC3/xHCI register offset, controller parameters, DMA addresses, interrupt routing and real
port/device behaviour remain R4 UNVERIFIED items.

## 5. Tests

`cargo test -p raven-usb-core` covers:

* device/configuration header/interface/endpoint round trips, all standard malformed classes,
  alt settings, endpoint count/address checks, class-specific preservation and the synthetic
  fixture;
* address pool order, reserve/release, invalid addresses and exhaustion at 127;
* setup packet direction/type/recipient, reserved bits, standard request constructors and the
  no-data length rule;
* full enumeration order and state, descriptor short/malformed failures, address release on
  configuration failure, second-enumeration state error, `SET_INTERFACE`, release and configuration
  count errors;
* hub attach/power/reset/enable/suspend/resume/detach/over-current, change bits and bounds;
* xHCI seam chain/endpoint/slot stamping, no-data sequences, full-ring rejection, completion
  decoding and stall classification.

`make check-fast` is the ticket gate.

## 6. S60 seam and board steps

S60 HID can consume `Configuration`, `Interface`, `EndpointDescriptor`, `ControlTransport` and the
`XhciSeam`. It should add its HID descriptor/report parsing in its own crate and use the class-
specific records this crate preserves. No HID code is added here.

Board steps (all UNVERIFIED):

1. Confirm the DWC3-to-xHCI register offset and controller parameters (R4).
2. Map BAR0, enable bus mastering, allocate the device-context/command/event/ERST memory and run
   the controller (R4).
3. Confirm interrupt 31/36 routing and the IMAN/ERDP handshake (R4).
4. Reset a port, attach a device, read its descriptors and assign an address; compare the captured
   `lsusb -v`/`dmesg` with the parsed values.
5. Confirm the transfer-ring chain/endpoint/slot encoding on real hardware.
6. Only then wire S60 HID and a real keyboard.

## 7. Provenance

The descriptor/setup/standard-request shapes are the public USB 2.0 definitions. No Linux, BSD,
GPL or vendor USB core code was copied, translated or closely paraphrased. No donor file was read
for this ticket beyond the R4 model/docs and the project fixtures; no `PROVENANCE.toml` import
entry is added. The synthetic fixture is our own construction.
