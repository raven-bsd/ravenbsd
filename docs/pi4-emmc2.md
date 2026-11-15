<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 EMMC2 SD path (J1/J2)

Status: model and driver skeleton. J1 covers the register, command/response, initialisation and
timing model; J2 adds the SDMA/ADMA2 descriptor model, bounce-window rules, the multi-block
transfer state and the `block:` request service. Every host test below is OBSERVED on the
development host; every real-card result is UNVERIFIED because no Pi 4 board log exists. The model
is clean-room Rust: the donor files were read for behaviour only and no code, comment or table
layout was copied.

## 1. What exists

| Piece | Path | Evidence |
|---|---|---|
| SDHCI register map, widths, bit fields, capabilities, quirks | `crates/raven-pi4/src/registers.rs` | OBSERVED by host tests |
| Command and transfer-mode encoding | `crates/raven-pi4/src/command.rs` | OBSERVED by host tests |
| Response, card status, CID and CSD parsing | `crates/raven-pi4/src/response.rs` | OBSERVED by host tests |
| SD card initialisation state machine | `crates/raven-pi4/src/init.rs` | OBSERVED by host tests |
| 1-bit/4-bit, 400 kHz/25 MHz/50 MHz plans and typed transitions | `crates/raven-pi4/src/timing.rs` | OBSERVED by host tests |
| SDMA/ADMA2 descriptors, END/VALID checks and the 4 GiB limit | `crates/raven-pi4/src/dma.rs` | OBSERVED by host tests |
| Transfer state, block progress and error mapping | `crates/raven-pi4/src/transfer.rs` | OBSERVED by host tests |
| EMMC2 device-tree lookup (base, SPI, DMA window) | `crates/raven-pi4/src/fdt.rs` | OBSERVED against the fixture |
| Driver register/clock seam | `userland/drivers/emmc2/src/lib.rs` | OBSERVED by host tests |
| `block:` request mapping and memory backend | `userland/drivers/emmc2/src/block.rs` | OBSERVED by host tests |
| Minimal DTB fixtures (hex text plus `.dts`) | `tests/fixtures/pi4-emmc2/` | OBSERVED; facts documented in its README |

The crate is `no_std`, allocation-free and `#![forbid(unsafe_code)]`. It never touches a register:
the driver skeleton performs the register sequence through the `Host` trait, and the tests use a
register fake or the in-memory block backend. The real MMIO grant, DMA engine, interrupt and
scheme entry are the board integration step.

## 2. EMMC2 facts used

| Fact | Value | Evidence |
|---|---|---|
| Compatible | `brcm,bcm2711-emmc2` | OBSERVED device tree |
| Register window | CPU `0xFE34_0000` + `0x100` after `emmc2bus` translation | OBSERVED fixture; B42 spike |
| Interrupt | GIC SPI 126 (GIC id 158), level high | OBSERVED device tree |
| Voltage | 3.3 V only; no 1.8 V, no UHS | OBSERVED B42/NetBSD quirk set; UNVERIFIED on silicon |
| Bus | 1-bit for identification, 4-bit for data | OBSERVED B42/NetBSD; UNVERIFIED on silicon |
| Speeds | 400 kHz identification, 25 MHz default, 50 MHz high speed | OBSERVED B42/NetBSD; UNVERIFIED on silicon |
| Clock source | mailbox clock id 12; 50 MHz fallback | OBSERVED B42; UNVERIFIED on silicon |
| DMA window | bus `0xC000_0000` -> CPU `0x0`, 1 GiB, firmware-edited | OBSERVED fixture; live value must come from the DTB |
| DMA engines | SDMA (32-bit address) and ADMA2; 32/64-bit descriptors selected by capabilities | OBSERVED B42/NetBSD; widths and capability bits UNVERIFIED on silicon |
| Quirks | capabilities register not trusted, no power 0, no high-speed host bit, 32-bit access | OBSERVED B42/NetBSD flags; UNVERIFIED on silicon |

The driver must read the live device tree: the fixture values are facts for the host tests, not a
licence to hard-code a Pi 4 address. `Emmc2Node::bus_address` returns `None` outside the DTB window,
which is where a bounce buffer is required.

## 3. Model coverage

* **Registers:** all offsets used by the card sequence, the correct 8/16/32-bit width, the
  capabilities field decoding, the interrupt status bits, the present-state inhibit and buffer
  bits, and the `Quirks::PI4_EMMC2` set.
* **Commands:** CMD0/2/3/6/7/8/9/12/16/17/18/24/25/55 and ACMD6/41/51, each with its response
  kind, data direction and multi-block property. Encoding covers the response-type field, CRC
  enable, data-present bit, transfer mode, block size and block count. A data transfer on a command
  that carries none, a missing transfer, a direction mismatch and an out-of-range block size or
  count are typed errors before a register write.
* **Responses:** R1/R1b card status, R3 OCR, R6 RCA, R7 voltage/pattern echo, 136-bit CID and CSD.
  CID decodes manufacturer, OEM, five-byte product name, revision and serial. CSD v1 and v2 decode
  capacity; a reserved structure or an overflowing field returns `None`.
* **Initialisation:** CMD0, CMD8, ACMD41 (bounded retries), CMD2, CMD3, CMD9, CMD7, CMD55/ACMD6 and
  CMD16. A CMD8 timeout falls back to the v1 (non-HCS) path; a CRC or other CMD8 failure does not.
  Card error bits, a missing APP_CMD, RCA 0, a reserved CSD structure and an out-of-order advance
  all stop with typed errors.
* **Timing:** the version-3 10-bit divider and the older eight-bit power-of-two divider; 400 kHz,
  25 MHz and 50 MHz plans from a 50 MHz base; typed identification -> default -> high-speed
  transitions and 4-bit-only-after-identification. The EMMC2 high-speed quirk suppresses the
  standard host-control bit.
* **DMA:** 32-bit and 64-bit ADMA2 descriptor encode/decode, VALID/END/INT/ACT validation, the
  zero-means-65536 length convention, 4-byte alignment, exactly one END bit on the last descriptor,
  the 4 GiB descriptor limit, 16-descriptor tables and the SDMA 32-bit address. Buffers and the
  descriptor table are mapped through the `dma-ranges` window; anything outside it returns a typed
  `BounceRequired` reason instead of a truncated address.
* **Transfers:** CMD17/18/24/25 selection from direction and block count, validated block size and
  32-bit LBA, command and data phases, PIO block progress, DMA completion, block-count mismatch,
  sticky failure and the command/data/ADMA/DMA error interrupt mapping.
* **Block service:** `raven-proto-block` `INFO`/`READ`/`WRITE`/`FLUSH` decoding, 512-byte-sector and
  range validation, read-only rejection, in-place read replies, `LIST` rejection (blockd owns the
  partition table), backend errno propagation and a host-testable memory backend.
* **Device tree:** the fixture parser rejects a missing node, a missing `reg`, a non-SPI or
  non-level-high interrupt and a missing DMA window.

## 4. Driver seam

`userland/drivers/emmc2` consumes `raven-pi4` through `Host`:

```text
read/write(Register)     -> the MMIO grant (raven_driver::MmioRegion)
now_ns()                 -> the monotonic clock
clock_hz()               -> platd mailbox clock id 12
dma_window()             -> the DTB / S21d DMA window
```

`probe` records the version, raw and effective capabilities, the clock source and the three timing
plans. `bring_up` resets the host, applies 3.3 V, sets the maximum timeout and programs the
identification clock with a bounded stable-clock poll. `init_card` drives the state machine, then
programs the 25 MHz default clock and four-bit host control. The fake host exercises the full
sequence, the busy ACMD41 retry, the v1 fallback, a card error, a missing APP_CMD, a command
timeout, a stuck reset and a clock that never stabilises.

The `block` module is the protocol seam:

```text
BlockService<D: BlockDevice>::handle(request, response)
    -> raven-proto-block Request decode and range checks
    -> BlockPlan (MultiBlockTransfer + CMD17/18/24/25 + DataTransfer)
    -> D: BlockDevice::read/write/flush  (MemoryDisk in the host tests)
```

`MemoryDisk` is a borrowed byte slice, so the tests need no allocator. The card backend that drives
the PIO loop or the SDMA/ADMA2 descriptor table implements the same `BlockDevice` trait; the DMA
plan (`plan_sdma`, `plan_adma2`) is ready for that backend but the register writes, interrupt wait
and scheme entry remain the integration step.

The following are deliberately **not** in this skeleton and belong to the integration ticket:
the `raven_driver` MMIO adapter and the `devd` service entry, the `platd` clock query wiring, the
interrupt path, the PIO/SDMA/ADMA2 register engine, the CMD6 high-speed switch and the `disk:`
scheme export.

## 5. Tests

```
cargo test -p raven-pi4
cargo test -p emmc2
make check-fast
```

The crate tests cover command/response parsing, the timeout and error paths, 4-bit versus 1-bit
setup, the clock divider table for both specification generations, the DTB fixture, the ADMA2/SDMA
descriptor encoding and validation, bounce-window boundaries and the transfer state/error paths.
The driver tests cover probe, bring-up, the full card sequence and its failure paths against the
register fake, the `block:` request matrix against the memory backend and command selection
(CMD17/18/24/25). No QEMU model exists for EMMC2, so there is no qtest; the board log is the only
remaining gate.

## 6. Board steps (UNVERIFIED)

Capture the UART line and the register values for each step. None of these has been run; no board
run was performed for this ticket.

1. **Controller identity:** host-controller version, capabilities (`0x40`), capabilities 2 (`0x44`),
   the live `emmc2bus` `ranges`/`dma-ranges`, the base clock returned for mailbox clock id 12, and
   whether ADMA2 is 32-bit or 64-bit.
2. **Card initialisation:** CMD0/CMD8 response, ACMD41 OCR and CCS, CID/CSD summary, RCA, SCR
   (four-bit supported), CMD6 high-speed result, final clock and bus width.
3. **Transfer log:** CMD17/18 read and CMD24/25 write, block size/count, transfer-complete and
   command-complete bits, block count and the CRC/timeout counters after the run.
4. **SDMA:** `DMA_ADDR`, transfer-mode DMA-enable, the buffer bus address and the 1 GiB limit.
   **ADMA2:** `ADMA_SYSTEM_ADDR`, the descriptor-table bus address and alignment, VALID/END/ACT
   words, every data descriptor address below 1 GiB, and whether a bounce buffer was used.
5. **blockd:** the driver's `disk:0` `INFO` geometry, a `READ`/`WRITE`/`FLUSH` round trip through
   blockd, the GPT scan, a root mount, and create/read/delete of a file.
6. **Card details:** model and capacity, 1-bit versus 4-bit, 25 versus 50 MHz, and the error/
   timeout counters after the run.

The first failing step is the evidence to attach; do not claim a later step from a partial log.

## 7. Provenance

Donor files read as behavioural references (no code copied, no `PROVENANCE.toml` entry added):

* `ravenbsd-resources/netbsd/sys/arch/arm/broadcom/bcm2835_emmc.c`
* `ravenbsd-resources/netbsd/sys/arch/arm/broadcom/bcm2838_emmc2_acpi.c`
* `ravenbsd-resources/netbsd/sys/dev/sdmmc/sdhcreg.h`
* `ravenbsd-resources/netbsd/sys/dev/sdmmc/sdhc.c`
* `ravenbsd-resources/netbsd/sys/dev/sdmmc/sdhcvar.h`
* `ravenbsd-resources/redox-base/drivers/storage/bcm2835-sdhcid/src/main.rs`
* `ravenbsd-resources/redox-base/drivers/storage/bcm2835-sdhcid/src/sd/mod.rs`
* `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm/boot/dts/broadcom/bcm2711.dtsi`
  (numeric facts only; the file is GPL-2.0 and no text is reproduced)
* `ravenbsd-resources/netbsd/sys/external/gpl2/dts/dist/arch/arm/boot/dts/broadcom/bcm2711-rpi-4-b.dts`
  (numeric facts only)

The fixture trees in `tests/fixtures/pi4-emmc2/` are our own minimal constructions; its README
names the source and revision of every numeric fact.

## 8. Open questions / risks

1. The EMMC2 capabilities register contents and the specification generation are UNVERIFIED. The
   driver records both and defaults to 50 MHz when the mailbox is silent.
2. The live firmware edits `dma-ranges` per SoC revision; the fixture documents one 1 GiB window.
   The model returns `BounceRequired` outside it, but the physical bounce allocator and copy path
   belong to the integration data engine.
3. The high-speed CMD6 switch is modelled as data and a typed transition but not executed by the
   driver skeleton; the 50 MHz step remains an integration test.
4. The command poll uses `now_ns`; a real adapter must implement it from the monotonic clock or a
   timeout becomes an infinite loop.
5. ADMA2 32-bit versus 64-bit and the descriptor-table alignment are UNVERIFIED until the live
   capabilities and the board log confirm them. The table's cache maintenance is S21d's job.
6. No interrupt, MMIO adapter, DMA engine or `disk:` scheme entry exists yet, so this ticket alone
   cannot boot from the card.
