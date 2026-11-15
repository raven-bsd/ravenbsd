<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RP1 MFD host model: GPIO/pinctrl, clocks and power (R2)

Status: host-side model only. **No Pi 5 hardware has been run.** The MFD addresses, pin count,
interrupts, pinctrl groups and clock rates are OBSERVED in the official Pi 5 firmware device tree;
their register encodings and any real-device access are UNVERIFIED. R2 does not touch an RP1
register and makes no hardware claim.

## 1. What the firmware device tree says

The RP1 node (`/axi/pcie@1000120000/rp1`) is a `simple-bus` with 2 address and 2 size cells behind
the BCM2712 root complex. Its relevant children:

| Block | Value (RP1-local) | Source |
|---|---|---|
| GPIO banks | `0xc0_400d_0000`, `0xc0_400e_0000`, `0xc0_400f_0000`, `0xc000` each | `gpio@d0000: reg` |
| GPIO pins | 54 (`gpio-ranges` count `0x36`); 54 line names | `gpio@d0000` |
| GPIO interrupts | `(0, 4)`, `(1, 4)`, `(2, 4)` | `gpio@d0000: interrupts` |
| Pinctrl groups | `uart0` on gpio14/15 (tx/rx) and gpio16/17 (cts/rts); `i2c0` on gpio0/1 | `gpio@d0000` children |
| Clocks | `clocks@18000` + `0x10038`, `#clock-cells = 1`, 13 assigned rates | `clocks@18000` |
| PIO | `pio@178000` + `0x20` | `pio@178000` |
| USB | `usb@200000` + `0x10_0000`, `usb@300000` + `0x10_0000` (both `disabled`) | `usb@*` |
| SRAM | `sram@400000` + `0x1_0000` | `sram@400000` |
| Firmware shmem | `sram@400000/shmem@ff00` + `0x100` | `shmem@ff00` |
| Firmware node | `raspberrypi,rp1-firmware`, `simple-mfd`, mailbox `0x5b` and shmem `0x5c` phandles | `/rp1_firmware` |

The clock IDs and the rates the firmware assigned (tree order):

| ID | Rate (Hz) | ID | Rate (Hz) |
|---|---|---|---|
| 0 | 1,000,000,000 | 10 | 153,600,000 |
| 1 | 1,536,000,000 | 12 | 200,000,000 |
| 3 | 200,000,000 | 6 | 100,000,000 |
| 9 | 125,000,000 | 13 | 50,000,000 |
| 16 | 125,000,000 | 31 | 1,000,000 |
| 4 | 61,440,000 | 32 | 200,000,000 |
| | | 29 | 50,000,000 |

The clock **names** and parent links are not in the device tree (the Linux `rp1-clk` driver has
them; no Linux checkout is present in this workspace), so the model starts every clock without a
parent and leaves the names UNVERIFIED.

There is **no RP1 power controller node** in this tree, and no RP1 child has a `power-domains`
property. The BCM2712 peripherals reference the SoC PM block (`watchdog@7d200000`,
`#power-domain-cells = 1`); RP1 power/reset is controlled by the RP1 firmware. R2 therefore models
the domain-ordering invariants only, with no register claim.

## 2. The model (`crates/raven-rp1`)

The crate is `no_std` outside tests, `#![forbid(unsafe_code)]`, and now depends on `raven-fdt-lite`
(no-allocating tree reader) and `raven-pci` (the validated `Region` type from R1).

### 2.1 `mfd` — the device-tree view

`Mfd::from_fdt` anchors on the first RP1 child it recognises (`rp1-gpio`, `rp1-clocks` or
`rp1-gem`), walks the MFD node's children and copies:

* **`GpioBlock`**: up to three bank windows, the pin count and up to three bank interrupts.
* **`Pinctrl`**: one `PinRule { pin, function, bias }` per `pins` entry in each pinctrl group;
  the parser rejects a group without `function`/`pins`, a pin name that is not `gpioN`, a pin
  outside the declared count, and a name longer than 16 bytes.
* **`ClockBlock`**: the register window and a `ClockTree` with the assigned IDs/rates. A tree with
  no assigned rates is accepted (the minimal B54 fixture); a mismatch or an odd `assigned-clocks`
  cell count is rejected, as is a phandle that is not the clock provider's.
* **Function windows**: the first `reg` of every other child, classified by node-name prefix
  (Ethernet, USB, MMC, DMA, ADC, PIO, SRAM, mailbox, clocks, GPIO, serial, other) with the
  `status` turned into `enabled`.
* **`FirmwareBlock`**: the mailbox window and the firmware shared memory, with the shmem reg
  translated through its parent `ranges` to the RP1-local address (`0xc0_4040_ff00`).

The addresses stay RP1-local; the caller translates them through the R1 `Rp1Window`. The tests do
that for the GPIO bank and the Ethernet window and assert the CPU addresses
(`0x1f_000d_0000`, `0x1f_0010_0000`).

### 2.2 `gpio` — pin ownership and pinctrl

A pin has exactly one owner. `claim`/`release` enforce the ownership; `configure` accepts plain
GPIO (`None`) on any claimed pin, but an alternate function must be one the device tree declares
on that pin (`Pinctrl::allows`). `set_high` requires an output and the owner. Every transition is a
named error (`AlreadyClaimed`, `NotOwner`, `FunctionNotOnPin`, `NotOutput`), and the rule table and
pin count are bounded (16 rules, 64 pins).

### 2.3 `clocks` — IDs, rates and gates

`ClockTree` holds up to 32 clocks. `set_rate` rejects a zero rate, a rate above the clock's
`max_rate_hz` and a rate above the parent's current rate. `enable` requires the parent enabled;
`disable` is refused while any child is enabled. Duplicate IDs and a full tree are errors. The
assigned rates from the tree start enabled with `max_rate_hz = u32::MAX` because the real limits
are UNVERIFIED.

### 2.4 `power` — domain invariants

`PowerModel` holds up to 16 domains. `power_on` requires the parent on; `power_off` requires no
users and no powered child; `acquire`/`release` track users and refuse an underflow; `reset` is
allowed only while the domain is off and unused. The transitions return `PowerAction` values so
the caller can log and sequence them. Domain IDs and parent links are caller data, not hardware
facts.

## 3. Fixtures

`tests/fixtures/pi5/pi5-rp1-mfd.dtb` extends the B54 tree with the MFD nodes above (the B54 files
are byte-identical). Two variants exercise the failure paths:

| Fixture | Expected |
|---|---|
| `pi5-rp1-mfd.dtb` | Full MFD: 3 GPIO banks, 54 pins, 3 interrupts, 6 pinctrl rules, 13 clocks, PIO/USB/SRAM windows, firmware mailbox + shmem. |
| `pi5-rp1-mfd-bad-pins.dtb` | `gpio-ranges` count 0 -> `MfdError::PinCount(0)`. |
| `pi5-rp1-mfd-bad-clock.dtb` | One assigned rate missing -> `MfdError::ClockCount { clocks: 13, rates: 12 }`. |

The fixture generator records the value source in `tests/fixtures/pi5/README.md`; no donor DTB is
committed.

## 4. Tests

`cargo test -p raven-rp1` covers:

* name storage (round trip, 16-byte limit, rejection instead of truncation);
* GPIO (single owner, owner-only release/configure, declared vs undeclared alternate functions,
  plain GPIO, output-only drive, out-of-range pins, full rule table, declared bias);
* clocks (ordered lookup, duplicate/full tree, rate limits and parent limit, zero rate, unknown
  clock/parent, parent-gate dependency, child-blocked disable);
* power (off at start, parent ordering, users, powered-child block, reset conditions, unknown
  IDs/parents, duplicate/full model);
* MFD (GPIO banks/pins/interrupts/pinctrl, clock window and 13 rates, function windows, firmware
  mailbox/shmem, pin-controller construction, translation through the R1 RP1 window, the minimal
  B54 tree, a missing RP1 node, and the two malformed fixtures).

`make check-fast` is the ticket gate; `cargo clippy -p raven-rp1 --all-targets -- -D warnings` must
be clean.

## 5. Board steps (all UNVERIFIED)

1. Confirm the RP1 MFD windows and the GPIO bank count against a config-space dump and a register
   read of a known GPIO register.
2. Confirm the GPIO pin count and the bank-to-pin mapping; the tree says 54, the banks are 3 x
   `0xc000`.
3. Confirm the pinctrl function numbers and the register layout behind `uart0`/`i2c0`; the tree
   gives names, not mux values.
4. Confirm the clock register layout, the clock names, the parent tree and the real rate limits;
   the assigned rates are only the rates the firmware chose.
5. Confirm which RP1 function the firmware mailbox/shared memory actually uses and its protocol
   (R1's frame layout stays a caller-supplied model).
6. Confirm the RP1 power/reset sequence, which is not described by this device tree.
7. Only then wire the MFD model into the Pi 5 platform path.

None of steps 1-7 has been run. There is no Pi 5 register dump, clock tree or board log in this
repository.

## 6. Donors and provenance

| Source | Revision | Licence | Use |
|---|---|---|---|
| `rpi-firmware/boot/bcm2712-rpi-5-b.dtb` | `ab5c7f86283ceb58b7adb269cae1d16092a31bf6` (tier C) | Broadcom firmware package | Read-only decoding of the RP1 node: GPIO banks/pins/line names, interrupts, pinctrl groups, clock IDs/rates, function windows, sram/shmem and the firmware node. Values only; no DTB copied or redistributed. |
| `rpi-firmware/boot/overlays/rp1-pio-uart.dtbo`, `pps-rp1.dtbo` | same | Broadcom firmware package | Listed only; not decoded for R2. |
| `netbsd/sys/external/gpl2/dts/dist/arch/arm64/boot/dts/broadcom/bcm2712.dtsi` | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | `GPL-2.0 OR MIT` | The NetBSD revision has no RP1/PCIe node; B54's value table was the reference for the existing nodes. Values only. |
| Linux `drivers/mfd/rp1.c`, `drivers/pinctrl/pinctrl-rp1.c`, `drivers/clk/clk-rp1.c` | not in this workspace | GPL-2.0 (tier C) | **Not read**: no Linux checkout is present. The register encodings and clock names are therefore unmodelled and marked UNVERIFIED. |

No donor code was copied, translated or closely paraphrased. There is no new `PROVENANCE.toml`
import entry because there is no imported code. The RP1 firmware blob is not in this workspace and
is not redistributed.
