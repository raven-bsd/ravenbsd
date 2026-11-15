<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 GENET v5 Ethernet model (O1)

Status: host model only. **No hardware result is claimed.** Every real-link step below is
UNVERIFIED until a board log exists.

## Register window

The firmware device tree places the GENET node at `/scb/ethernet@7d580000` with
`reg = <0x0 0x7d580000 0x0 0x10000>`; the `/scb` `ranges` entry translates that to CPU physical
`0xfd58_0000`. The MDIO child is `mdio@e14` with `reg = <0xe14 0x8>`, so the MDIO register is at
`0xfd58_0e14`. Interrupts are two GIC SPI specifiers, 157 and 158, both level-high.
OBSERVED: `tests/fixtures/pi4-genet/` values and the firmware DTB (`rpi-firmware` donor).

| Group | Offsets | Contents |
|---|---|---|
| System | `0x000` | revision, port control, RBUF/TBUF flush |
| Interrupt | `0x200` | L2 CPU status/clear/mask |
| RBUF | `0x300` | RX buffer control, TBUF size |
| UMAC | `0x800` | command, MAC0/MAC1, max frame, MIB, filters |
| MDIO | `0xe14` | MDIO command |
| RX DMA | `0x2000` | RX ring registers and internal descriptors |
| TX DMA | `0x4000` | TX ring registers and internal descriptors |
| Per-queue | `+0xc00 + 0x40*qid` | ring pointers/counters |
| DMA control | `+0x1040` | ring config, enable, burst, timeouts |

Revision: raw major 6 identifies GENET v5; the NetBSD driver normalises raw 0 to 1 and raw 5/6
down by one before requiring 5. The helper in `crates/raven-genet/src/regs.rs` follows that
behaviour. OBSERVED: `bcmgenet.c` attach path and `bcmgenetreg.h` revision fields.

## Descriptors and rings

- Descriptor size is 12 bytes: status, address low, address high.
- Descriptors are device-internal MMIO registers: RX at `0x2000 + 12*idx`, TX at
  `0x4000 + 12*idx`. Only packet buffers are DMA memory.
- Count 256, default queue 16, per-queue stride `0x40`.
- RX status: `OWN` bit 15, `EOP` bit 14, `SOP` bit 13, length bits 27:16, errors bits 4:0
  (`LEN`, `FRAME`, `RX`, `CRC`, `OVRUN`).
- TX status: `OWN` bit 15, `EOP` bit 14, `SOP` bit 13, `QTAG` bits 12:7, `CRC` bit 6, length bits
  27:16.
- Buffers are 2048 bytes; RX uses two-byte alignment (`RBUF_ALIGN_2B`); TX sets
  `SOP|CRC|QTAG` on the first segment and `EOP` on the last.
- Ring setup: `RING_BUF_SIZE = (256 << 16) | 2048`, start 0, end `256*12/4 - 1 = 767`; queue 16
  enabled; TX/RX DMA control enable; RX xon/xoff thresholds 5 / 16.
- Interrupt mitigation: TX after 10 packets; RX after 10 packets or 57 us (125 MHz/1024 ticks).
- `crates/raven-genet/src/ring.rs` models ownership, malformed descriptors and the 16-bit
  hardware counter wrap, and maps RX/TX status to the B39 `raven-proto-net` ring flags.

OBSERVED: `bcmgenetreg.h:91-180`, `bcmgenet.c` ring setup and interrupt paths.

## MDIO and the BCM54213PE

The MDIO command at `0xe14` carries `START_BUSY` bit 29, `READ` bit 27, `WRITE` bit 26, PHY
address bits 25:21, register bits 20:16 and data bits 15:0. The driver polls `START_BUSY` with a
10 us interval for at most 1000 polls (10 ms). The model in `mdio.rs` makes the budget explicit
and returns a typed timeout instead of spinning.

PHY address is 1. The identifier registers decode to OUI `0x18c086` and model `0x000a` for the
BCM54213PE; the helper compares the canonical OUI and model. Link resolution reads `BMSR` for the
link/autonegotiation bits and the Broadcom `AUXSTS` register bits 10:8 for the resolved
speed/duplex.

OBSERVED: `bcmgenetreg.h:80-86`, `bcmgenet.c` MDIO read/write, `miidevs`/`miidevs.h` OUI and
model, `brgphy.c` link status path.

## RGMII delays

The Pi 4 tree says `phy-mode = "rgmii-rxid"`: the PHY adds the RX clock delay; the MAC does not
add a TX delay. The PHY driver writes two shadow registers:

| Mode | `AUXCTL` misc `RGMII_SKEW_EN` (RX) | Shadow `0x1c` `GTXCLK_EN` (TX) |
|---|---|---|
| `rgmii` | off | off |
| `rgmii-id` | on | on |
| `rgmii-rxid` | on | off |
| `rgmii-txid` | off | on |

`crates/raven-genet/src/phy.rs` produces the three read-modify-write values: the `AUXCTL` shadow
misc write, the shadow `0x1c` clock-alignment write, and `EXT_RGMII_OOB_CTRL` (set `RGMII_LINK`,
`RGMII_MODE_EN`, clear `OOB_DISABLE`; set `ID_MODE_DISABLE` only for plain RGMII). The MAC side
also programs `SYS_PORT_CTRL = EXT_GPHY (3)`.

OBSERVED: `brgphy.c` `brgphy_bcm54xx_clock_delay`, `brgphyreg.h` AUXCTL/shadow bits,
`bcmgenet.c` `genet_update_link` and `genet_init_locked`.

## MAC address path

The resolver in `crates/raven-genet/src/mac.rs` tries, in order:

1. `local-mac-address`, then `mac-address`, from the GENET node (firmware-patched in the live DT);
2. `GET_MACADDRESS` (`0x00010003`) through the VideoCore mailbox, which `platd` exposes as
   `plat:mac`;
3. the existing `UMAC_MAC0`/`UMAC_MAC1` registers.

Only a unicast, non-zero, non-broadcast address is accepted. `UMAC_MAC0` is bytes 0..4
big-endian; `UMAC_MAC1` is bytes 4..6 big-endian. OBSERVED: `bcmgenet.c` `genet_get_eaddr` and
`genet_init_locked`, `bcm2711-rpi-4-b.dts` (no static MAC property; the firmware patches it),
`raven-plat` mailbox tag table.

## DMA windows and cache policy

The live Pi 4 DTB `/scb` node has `dma-ranges`:

| Bus | CPU physical | Size |
|---|---|---|
| `0x4_7c00_0000` | `0xfc00_0000` | `0x0380_0000` |
| `0x0` | `0x0` | `0x4_0000_0000` |

The parser keeps these windows and offers bus/CPU translation. Packet buffers must be allocated
inside a window. GENET is not documented as cache-coherent; the driver must treat RX/TX buffers
as non-coherent (Normal-NC or explicit cache maintenance) until a board log proves otherwise.
OBSERVED: firmware DTB `scb/dma-ranges`; UNVERIFIED for GENET specifically.

## DMA bounce rules

`crates/raven-genet/src/dma.rs` routes one 2 KiB packet buffer through the device-tree windows:

* a range fully inside one window is used directly with its translated bus address;
* a range outside every window, or crossing a window edge, requires a caller-supplied bounce
  buffer that is itself directly addressable; the returned mapping then has `bounce = true`;
* zero length, more than 2048 bytes, a zero address or a non-64-byte-aligned address is rejected;
* the policy is non-coherent, so a ToDevice buffer is cleaned and a FromDevice buffer is
  invalidated unless the platform declares the domain coherent.

The S21d ABI currently has no copy-bounce primitive, so the userspace driver must allocate the
bounce buffer through `DMA_ALLOC` before it uses this rule. The model states the requirement; it
does not pretend a copy happened.

## Userspace driver seam and frame path

`userland/drivers/genet` is a skeleton: `Seam` is the only place that will own MMIO and DMA.
The host-testable `Controller` performs the typed bring-up order (probe revision, program MAC,
program RGMII, plan rings) and rejects an out-of-order transition. The real `devd`/profile wiring,
MMIO mapping, DMA allocation, IRQ delivery and the frame pump are the board integration step.
`FrameBackend` adds the host-tested frame path:

* `RxPath`/`TxPath` keep an owner (`Free`, `Driver`, `Device`) for each of the 256 slots, validate
  completed statuses and free a malformed RX slot instead of stalling the ring;
* posting, completing and refilling use the wrap-aware indices; a still-owned descriptor is not
  advanced;
* `FrameBackend` takes packet buffers from the seam, queues TX frames and reclaims completed ones;
* `LinkTracker` debounces `BMSR`/`AUXSTS` samples (default two) and applies `Up`, `Down` and
  speed-change events to `UMAC_CMD` and `EXT_RGMII_OOB_CTRL`.



## Real-link steps (UNVERIFIED)

1. Boot the Pi 4 and read `SYS_REV_CTRL`: record raw major/minor and prove the v5 normalisation.
2. Read `EXT_RGMII_OOB_CTRL`, program `SYS_PORT_CTRL = 3`, and record both values.
3. Resolve the MAC (`local-mac-address`, then `plat:mac`, then UMAC) and read back
   `UMAC_MAC0`/`UMAC_MAC1`.
4. Run MDIO reads of registers 2 and 3; record the BCM54213PE OUI/model and the revision.
5. Program the `rgmii-rxid` AUXCTL/shadow writes; read both back.
6. Start autonegotiation and record `BMSR`/`AUXSTS` at link up, including speed and duplex.
7. Read `UMAC_CMD` after the link update and prove the `SPEED` field.
8. Enable the rings and prove RX/TX `OWN` transitions, then pass an ARP or ping through `netd`.
9. Unplug/replug the cable and record the debounced link-down/link-up transitions, `UMAC_CMD`
   speed changes and the interrupt counts.
10. Inject one malformed frame and prove the error counter increments while the RX slot is freed
    and refilled.
11. Force a packet buffer above the DMA window and prove the bounce path copies through a
    directly addressable buffer, or returns `ENOMEM` when no bounce buffer exists.
12. Queue enough TX frames to wrap the 256-slot ring and reclaim every completion.

## Donor files read

Behavioural references only; no code was copied or translated.

- `ravenbsd-resources/netbsd/sys/dev/ic/bcmgenet.c`
- `ravenbsd-resources/netbsd/sys/dev/ic/bcmgenetreg.h`
- `ravenbsd-resources/netbsd/sys/dev/ic/bcmgenetvar.h`
- `ravenbsd-resources/netbsd/sys/dev/mii/brgphy.c`
- `ravenbsd-resources/netbsd/sys/dev/mii/brgphyreg.h`
- `ravenbsd-resources/netbsd/sys/dev/mii/mii.h`
- `ravenbsd-resources/netbsd/sys/dev/mii/miidevs`
- `ravenbsd-resources/netbsd/sys/dev/mii/miidevs.h`
- `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm/boot/dts/broadcom/bcm2711.dtsi`
- `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm/boot/dts/broadcom/bcm2711-rpi-4-b.dts`
- `ravenbsd-resources/rpi-firmware/boot/bcm2711-rpi-4-b.dtb` (data only)

The B42 spike report and the B39/B12c/S21d reports were read for the integration contracts.
