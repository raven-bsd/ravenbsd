<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# USB Audio Class 1.0 model on the RP1 xHCI (R5, extended by R6)

Status: host-side model only. **No Pi 5 hardware has been run.** The descriptor/class-request
shapes are the public USB 2.0 and USB Audio Class 1.0 definitions; the device values, the real
feedback behaviour, packet sizes and every board result are UNVERIFIED. R6 adds the xHCI
Setup/Data/Status control-transfer sequence and the audiod PCM-ring seam; it does not edit or
import `audiod`, and the server/driver integration remains an explicit follow-up.

## 1. Transport

R4 modelled the two RP1 Synopsys DesignWare USB3 controllers (`snps,dwc3`) behind the BCM2712 PCIe
root complex: windows `0xc0_4020_0000` and `0xc0_4030_0000`, CPU `0x1f_0020_0000` /
`0x1f_0030_0000`, interrupts 31/36, host role, LPM and the Synopsys quirks. The USB DAC itself is
discovered by enumeration, not described by the device tree; the `pi5-rp1-xhci` fixture is the
controller transport for the tests.

## 2. The model (`crates/raven-usb-audio`)

The crate is `no_std` outside tests, `#![forbid(unsafe_code)]`, and depends only on the R4
`raven-rp1-xhci` rings. It touches no register.

### 2.1 `usb` — standard descriptors

`DeviceDescriptor`, `ConfigDescriptor`, `InterfaceDescriptor` and `EndpointDescriptor` encode and
parse the USB 2.0 fixed-length descriptors with explicit length/type/bounds checks. The endpoint
model decodes the transfer type (control/iso/bulk/interrupt), the isochronous sync type
(none/asynchronous/adaptive/synchronous), the usage type (data/feedback/implicit), the endpoint
address (number + direction, DCI encoding) and the `wMaxPacketSize` size/transaction fields. The
8-byte control `SetupPacket` decodes the direction and the class-interface request form.

### 2.2 `uac1` — the descriptor graph

`Uac1Config` parses a complete configuration set: the AC header, input terminal (USB streaming),
feature unit, output terminal (speaker), the AudioStreaming interface, AS general, format type I
(discrete rates), the isochronous IN data endpoint and the optional asynchronous feedback endpoint.
It validates the interface numbers, the AC/AS subclasses, the terminal/unit references
(input -> feature -> output), the AS terminal link, the channel count, the format fields, the
endpoint types/directions and the endpoint count. `encode` writes the same set with a recomputed
`wTotalLength` and AC total, so the tests round-trip it. A malformed/truncated set, a dangling
entity, a non-audio interface, a bulk data endpoint or a bad feedback endpoint returns a typed
`Uac1Error`.

### 2.3 `control` — feature-unit requests

`FeatureUnitState` handles the UAC1 class requests on the mute and volume controls: `SET_CUR`/
`GET_CUR` and `GET_MIN`/`GET_MAX`/`GET_RES`; `SET_MIN`/`MAX`/`RES` are read-only and unsupported.
Volume is a signed 16-bit value in 1/256 dB with a caller-supplied range; an out-of-range write is
stalled, a wrong entity/interface/selector or a wrong direction is unsupported. Mute is one byte
(0/1).

### 2.4 `stream` — the isochronous cadence

`PcmFormat` is channels/subframe bytes/bit resolution/rate (S16 stereo in the model). `Schedule`
emits one packet per microframe (high speed, 8000/s) or per frame (full speed, 1000/s) and computes
the exact packet frame count with a cumulative distribution, so 44.1 kHz alternates 5/6 frames and
sums to exactly 44,100 per second; 48 kHz sends 6 frames per microframe. The asynchronous feedback
value is the samples-per-frame ratio in UAC1 16.16 form (for example `0x0006_0000` at 48 kHz and
`0x0005_8333` at 44.1 kHz); packets above the endpoint's `wMaxPacketSize` are rejected.

### 2.5 `host` — the device model over the xHCI rings

`AudioHost` opens the operational alternate setting at a supported rate and maps the streaming
endpoint to its DCI (endpoint 1 IN -> 3). `queue(buffer)` builds a Normal TRB with IOC for the next
packet and pushes it into the R4 `TransferRing`; `device_complete(code)` queues a Transfer Event
into the `EventRing`, and `complete` reclaims the ring slot and the event in order. A full ring is
reported without consuming a stream packet; `feedback()` returns the feedback value. The DMA
addresses are caller data; no controller is programmed.

`control(setup, data, buffer)` queues the xHCI Setup/Data/Status sequence on a second transfer ring
(endpoint 0, DCI 1): the Setup TRB carries the 8 setup bytes as immediate data and the TRT field
(no data/IN/OUT), the optional Data TRB carries the direction and length, and the Status TRB carries
the opposite direction and IOC. The xHCI crate builds the sequence; an inconsistent length is
rejected. The UAC1 request is applied to the feature unit immediately and returned as `Response`;
`control_device_complete` queues the endpoint-0 Transfer Event and `control_complete` drains the
two/three TRBs and the event. Only one control transfer is in flight at a time (endpoint 0 is
serialised).

### 2.6 `pcm` — the audiod ring seam

`PcmSource` is the frame-aligned source the stream consumes: `frame_bytes`, `available_frames` and
`read`. It is implemented for the R4 `SliceSource` (plain memory) and, in the same crate, for
`raven-proto-audio`'s `RingConsumer` (the audiod driver ring). `queue_pcm(buffer, source)` checks
the source's frame size against the stream format, pre-checks the whole packet is available, copies
the packet into the host scratch (`last_packet`) and queues the TRB; a short source is an
`SourceUnderrun` and leaves the stream and ring untouched. The driver copies the same bytes into
the DMA buffer at `buffer` before ringing the doorbell.

## 3. Tests

`cargo test -p raven-usb-audio` covers:

* USB descriptors: round-trips, bad length/type/truncation, endpoint type/sync/usage/DCI,
  `wMaxPacketSize`, the setup packet and its direction/class form;
* UAC1: a full descriptor set round-trip (`wTotalLength` = 101, 119 bytes total), malformed
  length/type/truncation, dangling entity/AS link/interface number, bad channel/format/endpoint,
  zero sample rate, a small output buffer, the feature-unit control bits;
* control: mute read/write/stall, volume read/write/min/max/res, out-of-range, wrong
  type/recipient/entity/selector/direction, read-only range fields, limit validation/clamping;
* streaming: format validation, 48 kHz 6 frames/24 bytes, 44.1 kHz exact 5/6 distribution and
  44,100 frames/s, full-speed 48 frames/192 bytes, packet-too-large, feedback bytes, start/stop;
* host: open/close, alternate/rate checks, DCI 3, queue/complete order with Transfer Events, a full
  ring that does not consume the packet, bad ring size;
* control path: a mute request queues the 3-TRB sequence and returns `Ack`, a GET returns the model
  state, a no-data request uses 2 TRBs, a small control ring is `RingFull`, limits can be replaced,
  the Transfer Event/complete order;
* PCM seam: the audiod `RingConsumer` reads whole frames, `queue_pcm` consumes 24-byte packets from
  a 48-frame ring, the scratch packet holds the payload, a wrong frame size is rejected and a short
  source is an underrun;
* integration: the `pi5-rp1-xhci` fixture yields the two controllers and the host runs on their
  rings; the encoded descriptor set parses and opens.

No new fixture blob was added: the UAC1 device is the crate's synthetic test configuration (our own
construction, not a donor). The Pi 5 fixture is the R4 controller transport.

## 4. audiod integration follow-up (not done here)

The host-side seam now exists (`queue_pcm` over `raven-proto-audio`'s `RingConsumer` and the
control-transfer sequence), but the audiod/server integration is still the driver ticket's work:

1. Map the UAC1 stream format to `raven-proto-audio`'s `Format` (S16LE, channels, negotiated rate)
   and attach the driver-side `RingConsumer` to the PCM region.
2. Drive `AudioHost::open`, `queue_pcm` and the completion handling from the xHCI driver's
   transfer-event loop; map the `wMaxPacketSize`, feedback value and underruns into the ring header
   and the `GET_STATUS`/`DRAIN` paths.
3. Route the control requests through the driver's endpoint-0 ring and report the device answer;
   use `GET_MIN/MAX/RES` for the initial volume range instead of the model default.
4. The xHCI register access, doorbell and completion interrupt are still UNVERIFIED; R6 does not
   edit `userland/servers/audiod/**`.

## 5. Board steps (all UNVERIFIED)

1. Enumerate the USB DAC on the RP1 xHCI port and dump its device/configuration descriptors.
2. Confirm the UAC1 feature unit controls (mute/volume), the terminal types and the format type I
   rates of the actual device.
3. Confirm the alternate setting, the isochronous IN endpoint's `wMaxPacketSize` and the feedback
   endpoint (present or absent) and its format.
4. Confirm the packet cadence and the feedback value the device actually requests.
5. Confirm the xHCI control-transfer TRB sequence (Setup/Data/Status, IDT/TRT/DIR) on the board.
6. Confirm the DMA addresses, the 64-bit window and the event/transfer ring sizes with the R4
   controller.
7. Confirm the PCM ring seam: the audiod period/frame mapping, the scratch packet copy and the
   underrun accounting.
8. Run audiod with a 440 Hz WAV and capture the result (the M-Pi5 exit).
9. Only then claim USB audio works on the board.

None of steps 1-9 has been run. There is no Pi 5 USB descriptor dump, feedback trace or capture in
this repository.

## 6. Donors and provenance

| Source | Revision | Licence | Use |
|---|---|---|---|
| R4 report and `crates/raven-rp1-xhci` | merged R4 (`2e7a7579`) | project source | Controller windows/interrupts, the `TransferRing`/`EventRing`/`Trb` types and (R6) the control-transfer sequence. |
| R5 report and `crates/raven-usb-audio` | merged R5 | project source | Descriptor/control/stream/host model extended by R6. |
| `crates/raven-proto-audio` | merged | project source | The audiod PCM ring layout/consumer the `pcm` seam is implemented for. |
| `architecture/rpi5/01-hardware.md`, `03-network-and-sound.md` | v0.1 | project docs | USB-audio path and the M-Pi5 sound exit (440 Hz capture). |
| `tests/fixtures/pi5/pi5-rp1-xhci.dtb` | our fixture | project fixture | Two `snps,dwc3` controllers as the transport in the integration test. |
| USB 2.0 specification | public standard | - | Descriptor field order/lengths and the setup packet. |
| USB Audio Class 1.0 specification | public standard | - | AC/AS descriptor shapes, terminal/feature-unit entities, class requests, format type I and the 16.16 feedback form. |
| Linux `sound/usb/*`, `drivers/usb/host/xhci*.c` | not in this workspace | GPL-2.0 (tier C) | **Not read**: no Linux checkout is present. Device quirks are unmodelled; the standard Setup/Data/Status TRB sequence is from the xHCI 1.1 spec. |

No donor code was copied, translated or closely paraphrased. There is no new `PROVENANCE.toml`
import entry because there is no imported code. No firmware or device blob is included.

## 7. Deviations and notes

* The UAC1 test device is synthetic (stereo S16, 44.1/48 kHz) and is not a donor trace; no USB
  device is present in this workspace.
* The host model keeps the DMA address and the slot ID as caller data; the xHCI control-transfer
  sequence is modelled from the xHCI 1.1 spec, while the audiod server wiring and the real
  register/DMA access remain follow-ups.
* The crate has two internal dependencies (`raven-rp1-xhci`, `raven-proto-audio`) and one
  dev-dependency (`raven-fdt-lite`); no external crate was added.
