<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RP1 firmware-load handshake model (R7)

Status: host-side model only. **No Pi 5 hardware has been run.** The RP1 boot ROM and the
host-side load protocol are not public; the window offsets, doorbell/status encoding, chunk size
and timeouts are UNVERIFIED. No RP1 firmware blob is present, copied or redistributed: the tests
use a synthetic fixture and the real path/hash/licence remain a board step.

## 1. Why

P2 (PCIe + RP1) needs the firmware load before Ethernet/USB work. R1 enumerated the RP1 endpoint and
recorded the firmware policy (`architecture/rpi5/04-firmware-and-provenance.md`); R7 models the
handshake a driver must run without claiming the undocumented register behaviour.

## 2. The model (`crates/raven-rp1-fw`)

The crate is `no_std` outside tests, `#![forbid(unsafe_code)]`, and depends on `raven-rp1` for the
`FirmwareImage`/`Digest` metadata record.

### 2.1 `plan`

`Plan::new(blob_len, chunk_len)` validates the image and splits it into chunks; the last chunk may
be short. The model bound is 4096 bytes per chunk and 65535 chunks (both **model limits**, not
hardware facts). Offsets are per-chunk and overflow-safe.

### 2.2 `seam`

`FirmwareSeam` is the only hardware boundary:

| Operation | Meaning |
|---|---|
| `reset` | assert/release the device reset |
| `boot_ready` | poll the boot ROM |
| `write_chunk(offset, data)` | write a chunk into the load window |
| `doorbell(offset)` | ring the doorbell for the chunk |
| `status` | `Ready`, `Busy` or `Fault(code)` |
| `start` | start the loaded image |

The kernel seam will map these onto the real window; the model never guesses an offset or a
register encoding.

### 2.3 `loader`

`Loader` drives one seam operation per `step`:

```text
Idle --reset--> WaitingBootRom --boot_ready--> Transferring
Transferring: write chunk -> doorbell -> poll status (Ready advances)
              (Busy/timeouts are bounded; Fault stops)
all chunks --status Ready--> Verifying --status Ready--> start --> Running --> Done
any error --> Faulted (reset returns to Idle)
```

`Limits` bound the boot/status timeout polls and the busy polls; a device `Fault(code)` stops
immediately; a seam bus error stops with `Seam(Bus)`. `Loader::run` loops until `Done` with a
4096-step safety bound. The loader checks the `FirmwareImage` record (path/digest/size/licence),
the record/blob size and the plan/blob size before touching the seam.

### 2.4 `sim` and the synthetic fixture

`SimSeam` is an in-memory double for the tests: it copies chunk writes into a 256-byte image,
counts doorbells/resets, answers `Ready`/`Busy`/`Fault`, and can inject a reset/write/boot/status
bus error. The fixture `tests/fixtures/pi5/rp1-fw-synthetic.md` is a **synthetic 64-byte payload**
(a 0..63 ramp) with a synthetic path/hash/licence; it is not the official firmware and is not
redistributable firmware. The real path is recorded only as
`REAL_FIRMWARE_PATH_UNVERIFIED`; `BLOB_REDISTRIBUTED` is false.

## 3. Tests

`cargo test -p raven-rp1-fw` covers:

* plan validation (empty image/chunk, oversized chunk, too many chunks), exact/remainder chunking,
  offset/length and the maximum chunk;
* the fixture: recorded metadata, 64-byte synthetic image, constants (no real path invented, no
  redistribution), a complete run;
* the loader: the full synthetic load (4 chunks, 16 steps, image verified, started), one seam
  operation per step, a short last chunk, boot-ROM retries then `Timeout`, busy budget then `Busy`,
  device `Fault(code)`, reset/write bus errors, a status timeout budget, and record/blob/plan
  mismatches;
* the sim double: matching/oversized images, reset/start/doorbell state, status answer order and
  injected failures;
* `make check-fast` is the ticket gate.

## 4. Board steps (all UNVERIFIED)

1. Capture the real RP1 boot-ROM handshake on a board: reset, boot-ready poll, load-window
   offsets/size, doorbell and status encoding, start/verify.
2. Confirm the chunk size and the timeout/busy budgets the device tolerates.
3. Record the official firmware path, SHA-256, size and licence; the owner decides whether it may
   be redistributed. No blob is added to this repository.
4. Confirm whether the VideoCore boot firmware or the OS performs the load on the Pi 5.
5. Confirm the load-window mapping through the R1 PCIe/RP1 window and the firmware's DMA/access
   path.
6. Run the loader against the real device and capture the serial log; only then claim the load
   works.

None of steps 1-6 has been run. There is no Pi 5 load trace, firmware hash or serial log in this
repository.

## 5. Donors and provenance

| Source | Revision | Licence | Use |
|---|---|---|---|
| R1 report (`tickets/phil/reports/R1.md`), `crates/raven-rp1` | merged R1 | project source | RP1 enumeration, the firmware policy and the `FirmwareImage`/`Digest` metadata record. |
| `architecture/rpi5/02-bringup-plan.md` (P2), `04-firmware-and-provenance.md` | v0.1 | project docs | Load step, blob policy, provenance rules and the owner decisions. |
| RP1 firmware blob | not in this workspace | proprietary/UNVERIFIED | **Not present, not read, not copied.** The synthetic fixture is our own construction. |
| Linux/NetBSD RP1 firmware loaders | not in this workspace | GPL-2.0/BSD (unread) | **Not read**: no donor checkout with an RP1 loader is present; the protocol remains UNVERIFIED. |

No donor code was copied, translated or closely paraphrased. There is no new `PROVENANCE.toml`
import entry because there is no imported code; the blob is not redistributed.

## 6. Deviations

* The fixture is a **synthetic text file** (no binary, no real blob); the real path/hash/licence
  stay UNVERIFIED.
* The load-window offsets/doorbell/status encoding are behind `FirmwareSeam`, not guessed; the
  kernel seam will supply them.
* The chunk size (`4096`) and chunk-count (`65535`) bounds are model limits.
* The crate depends on the internal `raven-rp1` crate; no external dependency was added.
