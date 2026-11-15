<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Raspberry Pi 5 PCIe root complex and RP1 enumeration (R1)

Status: host-side models and the kernel seam. **No Pi 5 hardware has been run.** Values marked
OBSERVED come from the official firmware device tree (read through the B54 fixtures and the
donor DTB); they are not silicon evidence. Every hardware-dependent item is UNVERIFIED until a
board log exists. R1 does not load the RP1 firmware and makes no hardware claim.

## 1. What the firmware device tree says

The official `bcm2712-rpi-5-b.dtb` contains three `brcm,bcm2712-pcie` nodes. Only the third
(`/axi/pcie@1000120000`) is `okay` and carries the `rp1` child; the other two are `disabled`.
This is OBSERVED in the donor DTB and modelled by the B54 fixture `tests/fixtures/pi5/pi5-valid.dtb`.

| Item | Value (CPU physical) | Source |
|---|---|---|
| Controller register block | `0x10_0012_0000`, length `0x9310` | `pcie@1000120000: reg` |
| 32-bit memory window | PCI `0x0` -> CPU `0x1f_0000_0000`, `0xffff_fffc` | `pcie: ranges` |
| 64-bit prefetchable window | PCI `0x4_0000_0000` -> CPU `0x1c_0000_0000`, `0x3_0000_0000` | `pcie: ranges` |
| RP1 child window | local `0xc0_4000_0000` -> PCI `0x0` -> CPU `0x1f_0000_0000`, `0x41_0000` | `pcie/rp1: ranges` |
| RP1 mailbox | local `0xc0_4000_8000` + `0x4000` | `pcie/rp1/mailbox@8000` |
| RP1 UART | local `0xc0_4003_0000` + `0x100` (`disabled` in the firmware tree) | `pcie/rp1/serial@30000` |
| RP1 Ethernet | local `0xc0_4010_0000` + `0x4000` | `pcie/rp1/ethernet@100000` |
| PCIe / MSI interrupts | `pcie` and `msi` GIC entries plus an `msi-controller`/`msi-parent` pair; MSI is not modelled by R1 | `pcie: interrupts`, `msi-controller` |

The tree has **no ECAM range**. The `ranges` entries are memory apertures, not config windows. The
BCM2712 root complex therefore uses indirect config cycles, as the BCM2711 does.

## 2. The model (`crates/raven-pci`)

`raven-pci` is `no_std` outside tests and `#![forbid(unsafe_code)]`. It has no hardware access:
the only boundary is the `ConfigSpace` trait, which the kernel seam or a host double implements.

* **`region`** - validated half-open windows (`base`, `size`, `end`, containment, translation).
* **`bdf`** - a validated bus/device/function address and its 16-bit route form.
* **`ecam`** - an optional ECAM window with whole-bus bounds (1 MiB per bus, 4 KiB function
  space). The Pi 5 firmware tree declares none; the type exists for boards and QEMU that do.
* **`config`** - typed access over the 32-bit backend: identity, revision/class, header type,
  command bits, and a bounded capability walk. Missing functions read all ones, per PCI.
* **`indirect`** - the BCMstb index/data protocol. Bus 0 addresses the root header directly at
  `offset`; every other bus writes the bus/device/function word to `EXT_CFG_INDEX` (`0x9000`) and
  accesses `EXT_CFG_DATA + offset` (`0x8000`). The offsets are the NetBSD BCM2711 values and are
  UNVERIFIED for BCM2712; they are data so a board log can correct them without touching the
  protocol.
* **`bar`** - sizing (`save`, write all ones, read mask, restore, `size = !mask + 1`), 32-bit and
  64-bit memory BARs, IO BARs, read-back verification after programming, and a first-fit aligned
  window allocator.
* **`rp1`** - RP1 discovery: scan a bus range, skip functions that do not answer or whose BAR map
  is malformed, and return the unique function whose memory BAR fully covers the device-tree RP1
  PCI window. Two matches are an ambiguity error, never a silent first choice.
* **`sim`** - deterministic host doubles: a BAR-aware config space that reproduces the all-ones
  sizing mask and a BCMstb window that routes index/data accesses. These are models, not drivers.
* **`plan`** - the data the kernel seam consumes: controller registers, optional ECAM and RP1
  window, all validated before use.

### BAR assignment

A 64-bit BAR writes its upper half first and its low half second, so the endpoint never decodes a
new low half against a stale high half. Every program step reads the address back; a device that
does not store the value is a hard error. The allocator hands out addresses in ascending order at
the aperture's own alignment until the window is full.

### RP1 discovery

The endpoint is identified by the window it decodes, not by a hard-coded ID, because the RP1
vendor/device IDs are UNVERIFIED in this workspace. The discovery scans bus 1 by default; the
actual bus number and the firmware BAR assignment are UNVERIFIED until a board log.

## 3. Firmware-load model (`crates/raven-rp1`)

`raven-rp1` is `no_std` outside tests and `#![forbid(unsafe_code)]`. It never carries the blob.

* **Record** - `FirmwareImage` holds the path, SHA-256, size and licence. `UNRECORDED` has none of
  them, and the machine refuses to start until the record is complete. No RP1 firmware file is
  present in this repository; the record stays UNRECORDED until the bench supplies it.
* **Frame** - the mailbox envelope is decoded through a caller-supplied `Layout` (magic, version,
  sequence, length, status, optional checksum). The real encoding is not public; the crate does
  not guess one. Empty, bad-magic, bad-version, oversized, truncated and checksum-mismatched
  responses are rejected before the state machine sees them.
* **Transfer** - `TransferPlan` turns an image length and a chunk length into a deterministic
  chunk list; the last chunk may be short. Retries are counted per chunk and bounded.
* **Machine** - `Idle -> Identifying -> Enumerated -> Loading -> Verifying -> Ready`, with
  `Faulted` for a named failure and `reset()` back to `Idle`. Enumeration checks the expected
  vendor/device (or accepts any function when the profile is UNVERIFIED). Acknowledgements and
  rejections must name the chunk in flight; anything else is a fault. Malformed responses are
  terminal (a response that cannot be decoded must not be retried blindly); timeouts are bounded
  per step; a chunk rejection is bounded by the retry limit. An event that does not belong to the
  current state is an error and leaves the state unchanged.

The mailbox and register encodings, the chunk size and the boot-ROM handshake are **UNVERIFIED**.
The sequence follows `architecture/rpi5/02-bringup-plan.md` phase P2 and has to be validated
against a bench observation before a load is attempted.

## 4. Kernel seam (`kernel/src/arch/aarch64/pcie.rs`)

The seam maps the controller register block through the kernel's device-MMIO window, implements
`RegisterWindow` with bounded volatile accesses, builds the indirect config backend and calls
`raven_pci::discover`. The module is declared in `kernel/src/arch/mod.rs` (a shared-additive
wiring file) with `#[path = "aarch64/pcie.rs"]`, because `kernel/src/arch/aarch64/mod.rs` is
outside R1's Owns paths. It is compiled for aarch64 but not called: the Pi 5 platform path lands
in a later ticket, after a board log confirms the register offsets and the RP1 bus number.

## 5. Board steps (all UNVERIFIED)

1. Confirm the BCM2712 indirect config offsets (`0x8000`/`0x9000`) with a config-space dump; if
   they differ, change the `Layout`/`indirect` data, not the protocol.
2. Confirm the PCIe link training and that the RP1 endpoint appears at bus 1, device 0, function 0.
3. Read and record the RP1 vendor/device ID, class, header type and BAR sizes.
4. Confirm which BAR covers the `0x41_0000` RP1 window and whether firmware pre-programs it.
5. Enable memory decoding and bus mastering; prove an RP1 register read with a known value.
6. Obtain the official RP1 firmware file; record its path, SHA-256, size and licence.
7. Observe the real boot-ROM handshake and mailbox frame layout; fill in `frame::Layout` and the
   chunk size.
8. Run the transfer model against the observed responses; then prove the RP1 identity after the
   firmware starts.
9. Only then wire the seam and the machine into the Pi 5 platform init.

None of steps 1-9 has been run. There is no PCIe config-space dump, firmware hash, RP1 ID or
serial log in this repository.

## 6. Donors and provenance

| Source | Revision | Licence | Use |
|---|---|---|---|
| `rpi-firmware/boot/bcm2712-rpi-5-b.dtb` | `ab5c7f86283ceb58b7adb269cae1d16092a31bf6` (tier C) | Broadcom firmware package | Node names, `reg`/`ranges`, controller count and status, RP1 child window. Values only; no DTB or blob copied or redistributed. |
| `netbsd/sys/arch/arm/broadcom/bcm2838_pcie.c` | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | BSD-2-Clause (tier A) | Behaviour of the BCM2711 indirect config cycle and the `EXT_CFG_INDEX`/`EXT_CFG_DATA` offsets and field positions. No code copied. |
| `netbsd/sys/arch/arm/broadcom/bcm2838_pcie.h` | same | BSD-2-Clause (tier A) | The register offsets/field layout listed above. No code copied. |
| `netbsd/sys/external/gpl2/dts/dist/arch/arm64/boot/dts/broadcom/bcm2712.dtsi` (read by B54) | same | `GPL-2.0 OR MIT` | Node shape; the revision has no PCIe node. Values only. |
| Linux `drivers/pci/controller/pcie-brcmstb.c` | not in this workspace | GPL-2.0 (tier C) | **Not read**: no Linux checkout is present. The BCM2712 register programming beyond config access is therefore unmodelled. |

No donor file was copied, translated or closely paraphrased. `PROVENANCE.toml` gets no import
entry because there is no imported code. The RP1 firmware record is deliberately absent: recording
a path, hash or licence without the file would be a fabricated provenance claim.

## 7. Tests

`cargo test -p raven-pci` covers ECAM validation and bounds, config-space lane selection and
capability walking, indirect index/data routing (including the bus-0 direct path), BAR sizing,
64-bit BAR halves, programming with read-back, the aligned window allocator, RP1 discovery from
`tests/fixtures/pi5/pi5-valid.dtb` (including the unprogrammed-BAR case, a bar smaller than the
window, a wrong bus, two matches and a malformed neighbour), and the DTB-derived plan.

`cargo test -p raven-rp1` covers digest parsing, the unrecorded-firmware refusal, frame decoding
and every malformed-response class, chunking (exact multiple, remainder, limits), acknowledgement
ordering, retry and timeout budgets, the identity mismatch, verify failure, malformed-response
termination and reset.

`make check-fast` is the ticket gate. The kernel seam is additionally compile-checked with
`cargo clippy -p raven-kernel --target aarch64-unknown-none-softfloat -- -D warnings`.
