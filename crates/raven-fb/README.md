<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-fb

Host-tested Raspberry Pi firmware display-buffer and HDMI text-console core.

The crate encodes the VideoCore mailbox property tags that request a linear display buffer
(`SET_PHYSICAL_W_H`, `SET_VIRTUAL_W_H`, `SET_DEPTH`, `SET_PIXEL_ORDER`, `GET_PITCH`,
`ALLOCATE_BUFFER`), decodes the firmware response, and renders monochrome text through the public
`ScreenBuffer` type. `DisplayRequest` and `DisplayInfo` are the mailbox protocol value types.

The public API uses **display buffer** / **HDMI console** terminology. Mailbox property names and
donor file/function names keep their data-sheet spelling where the protocol requires it.

The crate is `no_std`, has no unsafe code, and does not touch mailbox registers, MMIO, cache
maintenance or hardware. The Pi 4 kernel glue owns the registers, HHDM translation, cache clean /
invalidate and the UART mirror; `docs/hdmi-console.md` describes that boundary.

E-Ink keeps its `Canvas` terminology in `raven-eink`; this crate is the HDMI path only.
