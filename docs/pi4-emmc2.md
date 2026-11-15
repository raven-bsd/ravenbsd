<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 EMMC2 SD path (J1-J4)

Status: model, driver and host-backed qtest. J1 covers the register, command/response,
initialisation and timing model; J2 adds the SDMA/ADMA2 descriptor model, bounce-window rules, the
multi-block transfer state and the `block:` request service; J3 adds the image/evidence kit; J4 adds
the real MMIO/DMA/IRQ seam, the `disk:` scheme entry, a deterministic test mode and the
`pi4-emmc2` qtest. Host tests and the host-backed qtest are OBSERVED; every real-card result remains
UNVERIFIED because no Pi 4 board log exists. The model is clean-room Rust: the donor files were read
for behaviour only and no code, comment or table layout was copied.

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
| Host register/clock seam, bring-up, card init and block service | `userland/drivers/emmc2/src/lib.rs` | OBSERVED by host tests |
| Pattern, checksum and marker | `userland/drivers/emmc2/src/pattern.rs` | OBSERVED by host tests and qtest |
| Real `MmioRegion`/`DmaRegion`/`Irq` seam and `disk:` scheme | `userland/drivers/emmc2/src/main.rs` | OBSERVED in host-backed test mode; board UNVERIFIED |
| `pi4-emmc2` qtest and probe fixture | `tests/fixtures/pi4-emmc2/**`, `tests/qtest/cases/pi4-emmc2.toml` | OBSERVED (exit 33) |
| Minimal DTB fixtures (hex text plus `.dts`) | `tests/fixtures/pi4-emmc2/` | OBSERVED; facts documented in its README |

The crate is `no_std`, allocation-free and `#![forbid(unsafe_code)]`. It never touches a register in
the host tests: the driver performs the register sequence through the `Host` trait, and the tests use
a register fake or the in-memory block backend. The real adapter uses `raven_driver`'s safe MMIO/DMA
types; the interrupt wait/ack path is compiled for the target and remains UNVERIFIED on silicon.

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
* **Driver entry:** the real `MmioHost` reads/writes SDHCI registers through `MmioRegion`, derives
  the monotonic clock, and the `CardDevice` maps the DMA buffer's bus address through the window,
  programs SDMA (`DMA_ADDR`, transfer mode, block size/count, argument, command), waits on `Irq`
  and feeds the normal/error status to `MultiBlockTransfer`. The `disk:` handler owns the scheme
  open-file table, checks read/write flags and delegates to the block service.
* **Test mode:** `args = ["test"]` selects a memory disk with a sector-stamped pattern and an FNV-1a
  checksum marker; the driver prints the backend, pattern checksum and marker before serving, and
  stops after a client flush. The `pi4-emmc2` qtest drives the real program, handler and block
  framing against that disk.
* **Device tree:** the fixture parser rejects a missing node, a missing `reg`, a non-SPI or
  non-level-high interrupt and a missing DMA window.

## 4. Driver seam

`userland/drivers/emmc2` consumes `raven-pi4` through `Host`:

```text
read/write(Register)     -> the MMIO grant (raven_driver::MmioRegion)
now_ns()                 -> CLOCK_MONOTONIC
clock_hz()               -> the launcher's clock argument; platd query is a follow-up
dma_window()             -> the DTB / S21d DMA window passed by the launcher
```

The program has two modes:

* **Real mode (default).** It maps handles 3/4/5 as MMIO/DMA/IRQ unless `mmio_handle=`,
  `dma_handle=` and `irq_handle=` override them, plus `mmio_size=`, `dma_size=`, `clock=`,
  `window_bus=`, `window_cpu=` and `window_size=`. It runs `init_card`, enables the command/
  transfer/error interrupts, keeps the four-bit default timing and serves `disk:` with SDMA over
  the mapped DMA buffer. The buffer's bus address is mapped back through the window and checked by
  `plan_sdma`; an out-of-window buffer fails with `EIO` instead of a truncated address.
* **Test mode (`test`).** It uses a memory disk, writes the sector-stamped pattern and checksum
  marker, prints the backend/pattern/marker lines, serves `disk:` and stops after a client flush.
  This is the mode the `pi4-emmc2` qtest runs.

The `disk:` handler opens path `0` (or the root), uses `OpenTable`, checks read/write flags, and
delegates `INFO`/`READ`/`WRITE`/`FLUSH` to `BlockService`; `LIST` is rejected because blockd owns the
partition table. The real platform launcher (devd/platform wiring) is a follow-up: the J3 profile
fragment is disabled until the driver entry is authorized for the real image.

The real SDMA/IRQ path is constructed and compiled, but has no hardware log; see the UNVERIFIED
board steps below. ADMA2 is modelled and tested in `raven-pi4`; the current real data path uses SDMA.

## 5. Tests

```
cargo test -p raven-pi4
cargo test -p emmc2
cargo xtask qtest --case pi4-emmc2 --arch aarch64
make check-fast
```

The crate tests cover command/response parsing, the timeout and error paths, 4-bit versus 1-bit
setup, the clock divider table for both specification generations, the DTB fixture, the ADMA2/SDMA
descriptor encoding and validation, bounce-window boundaries and the transfer state/error paths.
The driver tests cover probe, bring-up, the full card sequence and its failure paths against the
register fake, the `block:` request matrix against the memory backend, command selection
(CMD17/18/24/25), and the sector-stamped pattern/checksum/marker. The `pi4-emmc2` qtest boots the
aarch64 image, runs the real driver binary in host-backed mode, and checks the `disk:` scheme
through the block framing: info, marker checksum, 2048-sector pattern checksum, a scratch write,
flush, `svcd: settled` and `init exited (0)` (guest exit 33). No QEMU model exists for EMMC2, so the
real MMIO/IRQ/SDMA path has no qtest and the board log is the only remaining gate.

## 6. Board steps (UNVERIFIED)

Capture the UART line and the register values for each step. None of these has been run; no board
run was performed for this ticket.

1. **Driver launch:** the platform launcher passes the MMIO, DMA and IRQ handles and the window/
   clock values (or the defaults); capture `emmc2: card`, `emmc2: dma`, `emmc2: registered disk`
   and `emmc2: serving`, and the first failing line if the driver exits.
2. **Controller identity:** host-controller version, capabilities (`0x40`), capabilities 2 (`0x44`),
   the live `emmc2bus` `ranges`/`dma-ranges`, the base clock returned for mailbox clock id 12, and
   whether ADMA2 is 32-bit or 64-bit.
3. **Card initialisation:** CMD0/CMD8 response, ACMD41 OCR and CCS, CID/CSD summary, RCA, SCR
   (four-bit supported), CMD6 high-speed result, final clock and bus width.
4. **Transfer log:** CMD17/18 read and CMD24/25 write, block size/count, transfer-complete and
   command-complete bits, block count and the CRC/timeout counters after the run.
5. **SDMA:** `DMA_ADDR`, transfer-mode DMA-enable, the buffer bus address and the 1 GiB limit.
   **ADMA2:** `ADMA_SYSTEM_ADDR`, the descriptor-table bus address and alignment, VALID/END/ACT
   words, every data descriptor address below 1 GiB, and whether a bounce buffer was used. The
   current real data path uses SDMA; the ADMA2 plan is model-tested until the path adopts it.
6. **blockd:** the driver's `disk:0` `INFO` geometry, a `READ`/`WRITE`/`FLUSH` round trip through
   blockd, the GPT scan, a root mount, and create/read/delete of a file.
7. **Card details:** model and capacity, 1-bit versus 4-bit, 25 versus 50 MHz, and the error/
   timeout counters after the run.

The `pi4-emmc2` qtest is host-backed: it proves the program, scheme, block framing and pattern
paths, not the MMIO/IRQ/SDMA path. Attach the first failing board line; do not claim a later step
from a partial log.

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
   The real path fails with `EIO` outside it; there is no physical bounce allocator/copy yet.
3. The high-speed CMD6 switch is modelled as data and a typed transition but not executed by the
   driver; the 50 MHz step remains an integration test.
4. The real `now_ns` uses `CLOCK_MONOTONIC` and returns `u64::MAX` on a clock failure so a deadline
   fails rather than spins; that path has no board log.
5. ADMA2 32-bit versus 64-bit and the descriptor-table alignment are UNVERIFIED until the live
   capabilities and the board log confirm them. The current real data path uses SDMA; the ADMA2
   plan is model-tested.
6. The transfer layer uses 32-bit block addresses (SDHC/SDXC). A v1 SDSC card reached through the
   CMD8 fallback would need byte addressing in CMD17/18/24/25; that path is not modelled.
7. The real MMIO/DMA/IRQ seam is compiled but not booted. The platform launcher must pass the
   handles and window/clock values; the J3 real-image fragment stays disabled until that wiring is
   authorized.
8. The `pi4-emmc2` qtest is host-backed (memory disk); it does not prove the SDMA, interrupt or
   real-card paths.
