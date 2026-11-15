<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RP1 Ethernet host model (R3)

Status: host-side model only. **No Pi 5 hardware has been run.** The window, interrupt, PHY and
clock values are OBSERVED in the official Pi 5 firmware device tree; the GEM register offsets and
descriptor encoding are not public in this workspace and stay UNVERIFIED. R3 does not touch an
RP1 register and makes no hardware claim; B57 owns the driver and the board evidence.

## 1. What the firmware device tree says

The RP1 Ethernet child (`/axi/pcie@1000120000/rp1/ethernet@100000`) is:

| Property | Value | Source |
|---|---|---|
| Window | RP1-local `0xc0_4010_0000` + `0x4000`; CPU `0x1f_0010_0000` through the R1 window | `reg` |
| Compatible | `raspberrypi,rp1-gem`, `cdns,macb` (Cadence GEM) | `compatible` |
| Interrupt | id `6`, flags `4` (two-cell RP1 interrupt) | `interrupts` |
| Clocks | `pclk` id 12, `hclk` id 12, `tsu_clk` id 29, `tx_clk` id 16 | `clocks`, `clock-names` |
| PHY mode | `rgmii-id` | `phy-mode` |
| Local MAC | all zero (bootloader placeholder) | `local-mac-address` |
| Cadence hints | `aw2w-max-pipe = 8`, `ar2r-max-pipe = 8`, `use-aw2b-fill` | `cdns,*` |
| PHY reset | GPIO 32, active-low, duration 5 | `phy-reset-gpios`, `phy-reset-duration` |
| PHY | MDIO address 1, `brcm,powerdown-enable`, `led-modes = 8` | `ethernet-phy@1` |

The public architecture doc calls the MAC IP UNVERIFIED; the DTB compatible (`cdns,macb`) settles
that it is a Cadence GEM derivative. The register layout is still not in the tree.

## 2. The model (`crates/raven-rp1-eth`)

The crate is `no_std` outside tests, `#![forbid(unsafe_code)]`, and depends on `raven-fdt-lite`,
`raven-pci` (the R1 `Region`/`Rp1Window`) and `raven-rp1` (the MFD parser and `Name`).

### 2.1 `device` — the DTB node

`EthernetNode::from_fdt` finds `raspberrypi,rp1-gem` (or `cdns,macb`), reads the `reg` window,
the interrupt pair, the clock pairs and names, `phy-mode`, the local MAC, the `cdns` hints, the
reset GPIO/duration and the `ethernet-phy@N` child (address and power-down flag). It rejects a
missing node, malformed cell counts, a non-six-byte `local-mac-address`, a clock/name mismatch and
addresses that do not fit. The all-zero MAC is reported as `None`, not as a real address.

### 2.2 `mac` — addresses and the filter

`MacAddress` parses and prints `aa:bb:cc:dd:ee:ff`, and classifies unicast, multicast and
broadcast. `MacFilter` holds up to eight exact unicast entries and a 64-bit multicast hash table,
with `Normal`, `Promiscuous` and `AllMulticast` modes. Broadcast always passes; multicast uses the
hash bit except in `AllMulticast`; unicast must match an exact entry. The hash is the standard
IEEE CRC-32 over the six octets; **which six bits the RP1 GEM uses is UNVERIFIED**, so the model
takes the low six bits and the board bring-up can change the mapping.

### 2.3 `mdio` — clause 22 and the PHY machine

`read_command` / `write_command` encode the IEEE 802.3 clause 22 32-bit word after the preamble
(start `01`, opcode `10`/`01`, 5-bit PHY, 5-bit register, turnaround `10`); `decode_read` checks
the start/opcode/turnaround and returns the 16-bit data. `Mdio` is a transaction state machine
(`Idle -> Awaiting -> Complete/Faulted`) with a bounded timeout/retransmit budget; a malformed
response faults it. `decode_bmsr` reads the standard link/autonegotiation/remote-fault bits from
BMSR/BMCR. The RP1 MDIO clock divider and the PHY-specific registers are UNVERIFIED.

### 2.4 `ring` — descriptor rings

`Ring` is a fixed power-of-two array (up to 256) with host/device ownership. `submit` places a
descriptor at the tail with the device as owner; `complete` reclaims the oldest device-owned
descriptor in FIFO order and returns it host-owned; the ring wraps and reuses freed slots. A full
ring is an error, an empty completion returns `None`, and a slot the device has not released is
not visible. The typed descriptor (buffer, length, first/last/error flags) is a model: the real
32/64-bit GEM descriptor encoding is UNVERIFIED.

### 2.5 `link` — link and interrupts

`LinkMachine` runs `Down -> Negotiating -> Up { speed, duplex }`, with `Faulted` on a PHY fault or
an exhausted poll timeout budget. Events outside the current state are errors and leave the state
unchanged. `InterruptState` models pending causes (RX complete, TX complete, link change, error),
a mask and acknowledgement; only unmasked pending causes are active. The real register bits are
UNVERIFIED.

### 2.6 `regs` — the register map

`RegisterMap` keeps a 32-bit-aligned offset for each named register (control, status, MAC address,
filter, hash, MDIO, ring bases, interrupt status/mask/ack, PHY and global config) inside the
`0x4000` window. The offsets are **caller data**: the board bring-up fills them from the Cadence
GEM documentation or a trace; the model never guesses an offset.

## 3. Fixtures

The R2 `pi5-rp1-mfd` fixture's Ethernet node now carries the full set of properties above (the
B54 `pi5-valid` node stays minimal; both are used by the tests). The `pi5-rp1-mfd-mac` variant
carries a non-zero local MAC so the parser's address path is tested. The fixture values are
transcribed from the official firmware DTB; see `tests/fixtures/pi5/README.md`. The R2 malformed
variants keep the same Ethernet node.

## 4. Tests

`cargo test -p raven-rp1-eth` covers:

* the device node: the full MFD fixture (window, Cadence flag, interrupt, four clocks, RGMII-ID,
  zero MAC, pipe hints, reset GPIO/duration, PHY address/power-down), the minimal B54 node, a
  tree without the node and the mode-name mapping;
* MAC: parse/print/classify, malformed addresses, the standard CRC-32 vector, the hash bit, exact
  filtering, hash/promiscuous/all-multicast modes, duplicate/multicast/overflow rejection and
  removal;
* MDIO: read/write command encoding, malformed responses (start/opcode/turnaround), BMSR/BMCR
  decoding, read/write completion, timeout retransmit/fault, malformed-response fault and the
  busy/idle guards;
* rings: invalid capacities, fill/full, FIFO completion with ownership transfer, wrap over many
  cycles, slot reuse after wrap and out-of-range slots;
* link: autonegotiation to up (speed/duplex), link loss, bounded timeouts, PHY fault and restart,
  events outside their state; interrupt pending/mask/ack;
* registers: set/get/clear, alignment and window bounds, and one distinct slot per name;
* integration: the Ethernet window equals the R2 MFD `FunctionKind::Ethernet` window and
  translates to CPU `0x1f_0010_0000` through the R1 `Rp1Window`.

`cargo test -p raven-rp1` (the R2 tests) still passes; `make check-fast` is the ticket gate.

## 5. Board steps (all UNVERIFIED)

1. Confirm the GEM register offsets and fill the `RegisterMap`.
2. Confirm the interrupt id 6 mapping in the RP1 interrupt controller and the mask/ack bits.
3. Confirm the MDIO clock divider, the clause 22 timing and the PHY address 1 response.
4. Confirm the RGMII-ID delays and whether the board needs TX/RX tuning.
5. Confirm the PHY reset GPIO 32 polarity/duration and the power-down behaviour.
6. Confirm the descriptor 32/64-bit layout, ring sizes, coalescing and the DMA window.
7. Confirm the MAC filter hash bit selection and the exact-filter register format.
8. Only then wire the model into the B57 driver and the `netd` path.

None of steps 1-8 has been run. There is no Pi 5 register dump, MDIO trace or link log in this
repository.

## 6. Donors and provenance

| Source | Revision | Licence | Use |
|---|---|---|---|
| `rpi-firmware/boot/bcm2712-rpi-5-b.dtb` | `ab5c7f86283ceb58b7adb269cae1d16092a31bf6` (tier C) | Broadcom firmware package | Read-only decode of the RP1 Ethernet node: `reg`, compatible, interrupt, clocks, `phy-mode`, reset GPIO, `cdns` properties and the PHY child. Values only; no DTB copied. |
| `netbsd/sys/external/gpl2/dts/dist/arch/arm64/boot/dts/broadcom/bcm2712.dtsi` | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | `GPL-2.0 OR MIT` | The NetBSD revision has no RP1/Ethernet node; no R3 value came from it. Values only. |
| Linux `drivers/net/ethernet/cadence/macb_main.c`, `drivers/net/phy/*` | not in this workspace | GPL-2.0 (tier C) | **Not read**: no Linux checkout is present. The GEM register offsets, descriptor encoding and PHY quirks are therefore unmodelled and marked UNVERIFIED. |
| IEEE 802.3 clause 22 frame format, CRC-32 | public standard | - | Frame field order, BMSR/BMCR bits and the CRC-32 polynomial; implemented from the standard, not from a donor file. |

No donor code was copied, translated or closely paraphrased. There is no new `PROVENANCE.toml`
import entry because there is no imported code.
