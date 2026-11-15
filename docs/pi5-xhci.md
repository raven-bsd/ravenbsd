<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RP1 xHCI host-controller host model (R4)

Status: host-side model only. **No Pi 5 hardware has been run.** The USB windows, interrupts,
role and Synopsys properties are OBSERVED in the official Pi 5 firmware device tree; the xHCI
register/TRB/port fields are the public xHCI 1.1 standard; the DesignWare-to-xHCI register offset,
the controller parameters and every real-device result are UNVERIFIED.

## 1. What the firmware device tree says

The RP1 node has two USB children (`/axi/pcie@1000120000/rp1/usb@200000` and `usb@300000`):

| Property | Value | Source |
|---|---|---|
| Windows | RP1-local `0xc0_4020_0000` and `0xc0_4030_0000`, `0x10_0000` each; CPU `0x1f_0020_0000` and `0x1f_0030_0000` through the R1 window | `reg` |
| Compatible | `snps,dwc3` (Synopsys DesignWare USB3) | `compatible` |
| Role | `dr_mode = host` | `dr_mode` |
| Interrupts | id `31` and `36`, flags `1` | `interrupts` |
| LPM | `usb3-lpm-capable` | flag |
| Tuning | `snps,axi-pipe-limit = 8`, `snps,tx-max-burst = 8`, `snps,tx-thr-num-pkt = 2` | `snps,*` |
| Quirks | `dis_rxdet_inp3`, `enhanced-nak-fs`, `parkmode-disable-ss`, `parkmode-disable-fsls` | `snps,*` |

The node is the DesignWare **core**, not a native xHCI node: the xHCI host controller is the HCD
that the DesignWare driver instantiates. Where its register block starts inside the 1 MiB DWC3
window is not in the tree and is UNVERIFIED; the model takes the xHCI base as data.

## 2. The model (`crates/raven-rp1-xhci`)

The crate is `no_std` outside tests, `#![forbid(unsafe_code)]`, and depends on `raven-fdt-lite`,
`raven-pci` (`Region`) and `raven-rp1` (the MFD parser). It touches no register.

### 2.1 `device` — the DTB nodes

`UsbControllers::from_fdt` walks the tree for `snps,dwc3` nodes (up to two), parses the `reg`
window (through the RP1 parent cells), the two-cell interrupt, `dr_mode`, the LPM flag, the three
Synopsys tuning values and the four quirks, and records whether `status` disables the node. The
minimal MFD fixture yields two disabled nodes with only the window; the `pi5-rp1-xhci` fixture
yields both full nodes.

### 2.2 `regs` — the standard xHCI map

The offsets and fields are xHCI 1.1: capability registers (`CAPLENGTH` 0x00, `HCIVERSION` 0x02,
`HCSPARAMS1` 0x04, `HCSPARAMS2` 0x08, `HCCPARAMS1` 0x10, `DBOFF` 0x14, `RTSOFF` 0x18), the
operational set (`USBCMD` 0x00, `USBSTS` 0x04, `PAGESIZE` 0x08, `CRCR` 0x18, `DCBAAP` 0x30,
`CONFIG` 0x38), the port sets at `0x400 + 0x10 * (port - 1)`, the runtime `MFINDEX` and the
interrupter set at `RTSOFF + 0x20` (`IMAN`, `IMOD`, `ERSTSZ`, `ERSTBA`, `ERDP`). `Capability`
decodes `max_slots`, `max_interrupters`, `max_ports`, AC64/CSZ/PPC/PIND, the stream-array size
and the extended-capability pointer; `PortStatus` decodes and encodes PORTSC; the USBCMD/USBSTS
bits are constants.

### 2.3 `trb` — 16-byte commands and events

A TRB is a 64-bit parameter, a 32-bit status and a 32-bit control. The model encodes/decodes the
cycle bit (0), chain (4), interrupt-on-completion (5), the six-bit type (10..15), endpoint ID
(16..20), slot ID (24..31), the Normal transfer length (0..16) and the event completion code
(24..31). Type codes: Normal 1, Setup 2, Data 3, Status 4, Link 6, Enable Slot 9, Disable Slot 10,
Address Device 11, Configure Endpoint 12, Reset Endpoint 14, Stop Endpoint 15, Set TR Dequeue 16,
Reset Device 17, Transfer Event 32, Command Completion 33, Port Status Change 34. Constructors
cover the command, transfer and event forms used by the rings.

### 2.4 `ring` — command, transfer and event rings

Command and transfer rings are fixed power-of-two TRB arrays whose last slot is a Link TRB; when
the producer reaches the link it writes it and continues at index zero with the flipped cycle.
The model tracks in-flight commands/transfers, FIFO completion and the producer cycle. The event
ring is written by the controller and read by the host with independent producer/consumer cycles;
it reports `Full` instead of overwriting an unconsumed event, and `consumer()` is the value the
host writes to ERDP.

### 2.5 `port` — roothub state machine

`PortMachine` runs `NotConnected -> Polling -> Enabled`, with `Resetting -> Enabled` (reset
complete), `Enabled -> Suspended -> Resumed -> Enabled`, and `Inactive` on over-current or a fatal
error. Connect, enable, reset, link and over-current changes are tracked and acknowledged
(write-1-to-clear). `Roothub` owns up to eight ports and answers `any_change`/`enabled_count`.

### 2.6 `slot` — slots and endpoints

`SlotTable` implements `Disabled -> Enabled -> Addressed -> Configured`: Enable Slot allocates the
lowest free slot, Address Device starts endpoint 0, Configure Endpoint marks the slot configured
and `endpoint_enable` starts a configured endpoint. Endpoints run, halt, stop, reset (halted back
to running) and fail; invalid order, DCIs and transitions are errors.

### 2.7 `interrupt` — IMAN, ERDP and EHB

`Interrupter` models the pending bit (write one to clear), the enable bit, `IMAN` encoding, the
ERDP dequeue pointer (low three bits reserved) and EHB. An event sets pending and EHB; updating
ERDP clears EHB. Only pending-and-enabled is `active`.

## 3. Fixtures and tests

`pi5-rp1-xhci.dtb` carries both RP1 USB nodes with the full properties; the R2/R3 MFD fixtures
keep their USB nodes disabled and minimal (R2's tests still assert that), and the B54 fixtures are
unchanged. `cargo test -p raven-rp1-xhci` covers:

* the device nodes (two full controllers, the disabled MFD variant, a tree without USB nodes,
  the `dr_mode` mapping);
* the register decode (capability/PORTSC/HCSPARAMS2/reserved offsets and the standard bit
  values);
* TRB word round-trips, type codes, command/event field placement, invalid slots/endpoints;
* command/transfer ring wrap and Link TRB, FIFO completion, full rings;
* event-ring FIFO, cycle flips, full without overwrite;
* port connect/reset/suspend/resume/disconnect/over-current and wrong-event rejection; roothub
  bounds/changes;
* slots (enable/address/configure/disable/full), endpoint run/halt/stop/reset/error and invalid
  transitions;
* interrupter pending/enable/ack, IMAN encoding, ERDP/EHB;
* integration: the USB windows equal the R2 MFD `FunctionKind::Usb` windows, translate to CPU
  `0x1f_0020_0000`/`0x1f_0030_0000`, and a capability/roothub/slot/ring bring-up plan runs end to
  end.

`cargo test -p raven-rp1` still passes; `make check-fast` is the ticket gate.

## 4. Board steps (all UNVERIFIED)

1. Confirm the xHCI register offset inside the DWC3 window and the DWC3 core configuration.
2. Read the capability registers and confirm MaxSlots, MaxPorts, MaxInterrupters, AC64 and the
   context size.
3. Map BAR0 through the R1 PCIe window, enable bus mastering and confirm MMIO reads.
4. Allocate the device-context array, command ring, event ring and ERST; program DCBAAP, CRCR and
   the interrupter registers; run the controller (USBCMD.RS) and watch USBSTS.
5. Confirm interrupt 31/36 routing (MSI or INTx) and the IMAN/ERDP handshake.
6. Reset the ports and confirm PORTSC connect/enable/link behaviour for the USB2 and USB3 ports.
7. Confirm the 64-bit DMA window and the context/TRB memory layout.
8. Only then wire the model into the R5 USB-audio path (xHCI + UAC1/UAC2) and test a real DAC.

None of steps 1-8 has been run. There is no Pi 5 register dump, xHCI trace or USB enumeration log
in this repository. The R5 USB-audio integration is an explicit follow-up.

## 5. Donors and provenance

| Source | Revision | Licence | Use |
|---|---|---|---|
| R1 report (`tickets/phil/reports/R1.md`), R2 report (`R2.md`), R3 report (`R3.md`) | this branch | project docs | PCIe/RP1 window, MFD function windows, fixture contract and the donor posture. |
| `architecture/rpi5/01-hardware.md`, `03-network-and-sound.md` | v0.1 | project docs | RP1 USB placement and the USB-audio path/exit. |
| `rpi-firmware/boot/bcm2712-rpi-5-b.dtb` | `ab5c7f86283ceb58b7adb269cae1d16092a31bf6` (tier C) | Broadcom firmware package | Read-only decode of the two USB nodes: windows, compatible, interrupts, role, LPM, tuning and quirks. Values only; no DTB copied. |
| `netbsd/sys/external/gpl2/dts/dist/arch/arm64/boot/dts/broadcom/bcm2712.dtsi` | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | `GPL-2.0 OR MIT` | Confirmed it has no RP1/USB node at this revision; no R4 value came from it. |
| Linux `drivers/usb/host/xhci*.c`, `drivers/usb/dwc3/*` | not in this workspace | GPL-2.0 (tier C) | **Not read**: no Linux checkout is present. The DesignWare offset, controller parameters and quirks handling are unmodelled/UNVERIFIED. |
| xHCI 1.1 specification | public standard | - | Register offsets/fields, TRB layout and type codes, PORTSC and port/endpoint/slot states. |

No donor code was copied, translated or closely paraphrased. There is no new `PROVENANCE.toml`
import entry because there is no imported code.
