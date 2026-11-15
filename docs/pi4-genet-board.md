<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 GENET board evidence

Status: evidence kit and host-tested seam. No Pi 4 board run was performed for O4; every real-link
value below is UNVERIFIED. The host tests are the observed evidence.

## What to capture

The board record is printed by the driver after `Controller` reaches `Ready`. Its six stable
markers are:

| Marker | Fields | Meaning |
|---|---|---|
| `GENET-REV` | `raw`, `major`, `minor`, `v5` | raw `SYS_REV_CTRL` and the donor-normalised version |
| `GENET-PORT` | `ctrl` | raw `SYS_PORT_CTRL` after the external-GPHY write |
| `GENET-MAC` | `source`, `addr` | selected address and whether it came from the DTB, mailbox or UMAC |
| `GENET-UMAC` | `mac0`, `mac1` | read-back of `UMAC_MAC0`/`UMAC_MAC1` |
| `GENET-PHY` | `id1`, `id2`, `oui`, `model`, `bcm54213pe` | BCM54213PE identifier read-back |
| `GENET-RGMII` | `oob`, `auxctl`, `shadow` | `EXT_RGMII_OOB_CTRL`, AUXCTL 0x18 and shadow 0x1c read-backs |

The script checks all six and prints the commit plus the log SHA-256:

```sh
scripts/pi4-genet-evidence.sh /tmp/pi4-genet.log
scripts/pi4-genet-evidence.sh --capture /dev/ttyUSB0 /tmp/pi4-genet.log 60
```

`--capture` uses `stty -F` and `timeout cat`; the board owner can instead use the J3 serial helper
when it lands. A log without every marker exits 1 and prints `genet-evidence: FAIL`.

## Twelve-step checklist

Run the steps in order. A later step is not evidence if an earlier step is missing.

1. **Build and boot the image.** Build the aarch64 kernel/image and boot the Pi 4 with the UART
   captured at 115200 8N1. Confirm the kernel banner. Do not claim a GENET result yet.
2. **Confirm the live node.** Read `/scb/ethernet@7d580000` from the live DTB: `reg` translated to
   `0xFD58_0000` + `0x10000`, `mdio@e14`, PHY address 1, `phy-mode = "rgmii-rxid"`, SPI 157/158.
   Expected markers: none; record the DTB hash and values in the log.
3. **Revision.** Run the driver bring-up until `Ready`. Expected: `GENET-REV raw=... major=6
   minor=... v5=true`. Raw major 6 is the Pi 4 GENET v5 numbering; raw 5 is the older value.
4. **Port control.** Expected: `GENET-PORT ctrl=0x00000003` (external GPHY). Also record
   `EXT_RGMII_OOB_CTRL` from step 8.
5. **MAC source.** Expected: `GENET-MAC source=dtb addr=...` when the firmware patched the tree,
   or `source=mailbox` when `platd` returned `GET_MACADDRESS`; `source=hardware` is the last
   fallback. A zero/broadcast/multicast address must be rejected.
6. **UMAC read-back.** Expected: `GENET-UMAC mac0=... mac1=...` matching the step-5 address
   (bytes 0..4 big-endian in `MAC0`, bytes 4..6 in `MAC1`).
7. **PHY identity.** Expected: `GENET-PHY id1=0x600d id2=0x84a0 oui=0x18c086 model=0x0a
   bcm54213pe=true` (revision may vary in the low nibble). A different OUI/model is a board
   mismatch, not a link failure.
8. **RGMII read-back.** Expected: `GENET-RGMII oob=... auxctl=... shadow=...`; for `rgmii-rxid`
   the AUXCTL RX-skew bit is set and the shadow GTX-clk bit is clear. Exact values are UNVERIFIED.
9. **Link up.** Start autonegotiation and record `BMSR`/`AUXSTS`, speed, duplex and the
   `UMAC_CMD` speed field. This step is currently a serial/manual check; it has no `GENET-*`
   record marker yet.
10. **Interrupts and reclaim.** Generate RX/TX traffic and record `INTRL2_CPU_STAT`, the reclaim
    counts, `INTRL2_CPU_CLEAR`, and the `ServiceReport` fields. This is a serial/manual check.
11. **Filters.** Program broadcast, the station address and two multicast addresses. Prove listed
    frames are accepted and an unlisted multicast is dropped; then set promiscuous mode and prove
    the drop disappears. This is a serial/manual check.
12. **Error and bounce paths.** Inject a malformed RX frame (error counter, slot freed/refilled),
    force a buffer above the DMA window (bounce or `ENOMEM`), and wrap the 256-slot TX ring.
    These are serial/manual checks; the host tests cover the expected paths.

If a step fails, attach that log and stop; do not claim later steps.

## UNVERIFIED values

These must come from the board log, not from the fixture:

- live `/scb` `ranges`/`dma-ranges` and the GENET/MDIO base addresses;
- `SYS_REV_CTRL` raw major/minor and the v5 normalisation;
- `SYS_PORT_CTRL` and `EXT_RGMII_OOB_CTRL` read-back;
- whether `local-mac-address`, `GET_MACADDRESS` or the UMAC registers supplied the MAC;
- `UMAC_MAC0`/`UMAC_MAC1` read-back;
- BCM54213PE identifier and revision;
- AUXCTL 0x18/shadow 0x1c values for `rgmii-rxid`;
- link speed/duplex and the `UMAC_CMD` speed field;
- interrupt mask polarity, reclaim counts and `INTRL2_CPU_CLEAR`;
- filter slot mask and promiscuous behavior;
- whether the TX descriptor needs software `OWN` or the DMA consumer index;
- recovery thresholds and ring-reset behavior.

## Profile follow-up (not edited in O4)

J3 owns `profiles/pi4*` and `scripts/pi4-serial.sh`; O4 does not edit them. The current
`profiles/pi4.toml` has no GENET program or service entry. The minimal exact initfs fragment to
add the built driver is:

```toml
# profiles/pi4-genet.d/10-genet.toml
[[initfs.programs]]
package = "genet"
path = "/sbin/genet"
```

The production service entry still needs a scheme/readiness contract; the O4 driver exits after
printing its skeleton message, so a truthful temporary manifest is:

```toml
# profiles/pi4-genet.d/genet.manifest
name = "genet"
exec = "/sbin/genet"
needs = ["logd", "scheme:plat"]
readiness = "exit"
restart = "never"
max_restarts = 0
```

This fragment is a follow-up for the profile owner. Wiring the real MMIO/DMA/IRQ handles, the
`plat:` MAC and the `netd` frame service is the board integration step, not an O4 edit.

## Provenance

No donor code was copied. The GENET donor files are listed in `docs/pi4-genet.md`; the O4 work
re-read the O2/O3 reports, the S21d report, `crates/raven-driver` and the EMMC2 host seam. The
fixtures and this document are original BSD-3-Clause work.
