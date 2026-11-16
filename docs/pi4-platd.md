<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 platd platform handles (P12)

Status: the `platd` capability plan, the fake P8-launch qtest and the `plat:` scheme transcript
are OBSERVED on the development host. The real hardware launch and the Pi 4 board are UNVERIFIED.
The production profile edit is documented below but is **not usable yet** with P8's generic
launcher: P8 transfers one BAR, one DMA and one IRQ, while platd declares three MMIO windows plus
DMA. No board result is claimed.

## 1. Resource table

`platd`'s hardware backend needs four distinct capabilities. The supervisor passes each as
`name=handle:length`; the parser rejects a missing/duplicated handle, a reserved slot (0/1/2) or
a window shorter than the register block.

| Resource | Pi 4 firmware window | Minimum length | Argument |
|---|---|---|---|
| VideoCore mailbox | `0xFE00_B880` + `0x40` | `0x40` | `mailbox=HANDLE:64` |
| BCM2711 GPIO | `0xFE20_0000` + `0xB4` | `0xB4` | `gpio=HANDLE:180` |
| RNG200 | `0xFE10_4000` + `0x28` | `0x28` | `rng=HANDLE:40` |
| Mailbox DMA buffer | supervisor DMA pool | `0x100` | `dma=HANDLE:256` |

The handles must be distinct and at least 3; a fresh P8 child has self (0), initfs (1) and
namespace (2). The hardware backend maps only these windows. A direct-DMA or clock resource is
not part of this ticket.

`platd`'s `--fake` backend is selected with `fake`/`fake=1` and does not map any capability; the
qtest uses it because QEMU `virt` has no BCM2711 mailbox/GPIO/RNG200.

## 2. P8 launch and the current gap

`devd dtb launch` selects a platform node, creates its declared BAR/DMA/IRQ grants, transfers
them in order and waits for the driver. The P8 table's selectors are one `bar=<n>`, one
`resources=dma+irq` and the generated `bar`/`bar_size`/`dma`/`irq` tokens.

`platd` needs **three** MMIO handles (mailbox, GPIO, RNG) plus one DMA handle. The current P8
launcher cannot create or transfer that set, and the driver table cannot name the windows. This
is a proven launcher/table gap, not a platd bug:

```
P8 now:   BAR (slot 3), DMA (slot 4), optional IRQ (slot 5)
platd:    mailbox (H1), gpio (H2), rng (H3), dma (H4), all distinct and >= 3
```

The P12 fake qtest proves that `devd` launches `/sbin/platd` from the driver table and that the
`plat:` scheme answers; it does not claim the hardware window handoff. The host tests prove the
four-window parser and its rejection paths.

### Required P8/REL3 extension (outside P12 Owns)

Either of these makes the production entry expressible:

1. Extend the P8 platform plan/table to accept repeated or named MMIO selectors (for example
   `mailbox=`, `gpio=`, `rng=`) and transfer one handle per declared window; or
2. Define a platd-specific combined platform window and let platd split the sub-windows by
   offset. This is broader than the per-window grant and is not implemented in P12.

P12 does not edit `userland/servers/devd/**` or `profiles/pi4*`; the exact profile/table edit is
below for REL3 after the P8 extension lands.

## 3. Scheme transcript

The client starts after devd's readiness gate (`scheme:plat`) and exercises the existing `plat:`
scheme. The fixture runs devd under the `pcid` service name because init's root-device grant is
reserved for `pcid` (the same workaround as the P8 fixture); the production profile needs the
P10/REL3 grant policy.

```
devd: dtb driver platd compatible=brcm,bcm2711-genet-v5 device=/scb/ethernet@7d580000
devd: dtb launch platd bar=0 size=0x10000 dma=0 irq=0
platd: ready backend=fake
svcd: pcid up
pi4-platd: model=0x00002711
pi4-platd: revision=0x00c03114
pi4-platd: serial=0x0000000012345678
pi4-platd: mac=02:00:00:00:00:01
pi4-platd: clock12=50000000
pi4-platd: power0=1
platd: claim pin=17
pi4-platd: gpio claim ok
platd: conflict pin=17
pi4-platd: gpio conflict ok
pi4-platd: gpio config ok
platd: release pin=17
pi4-platd: rng=ad4df30bae771bdc
pi4-platd: done
svcd: plat-client up
svcd: settled
init exited (0)
```

These are the deterministic fake values; they are not board register readings. The qtest case is
`tests/qtest/cases/pi4-platd.toml`, x86_64, with 22 matched expectations and no forbidden panic.

## 4. REL3 profile edit (target, after P8 extension)

The Pi 4 release keeps `platd` disabled until the P8 launcher can transfer the four handles.
When it can, REL3 should apply the following. The compatible string must name the platd platform
node in the release DTB.

`userland/servers/devd/drivers.toml`:

```toml
[[driver]]
name = "platd"
compatible = "raven,pi4-platform"
exec = "/sbin/platd"
args = ["mailbox=3:64", "gpio=4:180", "rng=5:40", "dma=6:256"]
```

`profiles/pi4-release.d/devd.manifest`:

```toml
name = "devd"
exec = "/sbin/devd"
args = ["devd", "dtb", "launch"]
needs = []
provides = ["scheme:plat", "scheme:dev"]
readiness = "scheme"
ready_timeout_ms = 60_000
restart = "never"
max_restarts = 0
enable = true
```

`profiles/pi4-release.d/platd.manifest` must stay disabled (or be removed): devd launches platd,
so a second svcd service would register a duplicate `plat:` server.

`profiles/pi4-release.d/genet.manifest` keeps `needs = ["logd", "scheme:plat"]`; netd waits for
`scheme:nic` (REL4d). The devd readiness gate above is `scheme:plat`; the `nic:` sequencing is
REL4d/P8 work, not P12.

The handle numbers in the table are illustrative until the P8 extension defines the transfer
order. The parser accepts any distinct handles >= 3 and the minimum lengths.

## 5. Tests and acceptance

```sh
cargo nextest run -p raven-platd
cargo nextest run -p raven-devd
cargo xtask qtest --case pi4-platd --arch x86_64
cargo check -p raven-platd --target aarch64-unknown-none-softfloat
make check-fast
```

The package is `raven-platd`; the binary is installed as `/sbin/platd`. The ticket's
`cargo check -p platd` spelling has no matching package.

The resource plan tests cover the four windows, P8 token passthrough, missing/short/duplicate/
reserved handles and unknown arguments. The qtest covers the devd launch, `plat:` scheme and
client output. The board and the production window handoff remain UNVERIFIED.

## 6. Provenance

No donor code was read or copied. The platd and fixture code is first-party BSD-3-Clause work;
the P8 fixture DTB is reused read-only from `tests/fixtures/pi4-devd/`.
