// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `vis.h` flag and result constants (NetBSD values).
//!
//! The values are those of the installed `include/vis.h`
//! (`libc/ravenlibc/include/vis.h`, derived from `netbsd:include/vis.h`). The C65 report
//! records that the original ticket expected the FreeBSD `contrib/libc-vis` values; the
//! FreeBSD checkout is absent, the donor used here is NetBSD, and donor and installed header
//! agree. `tests/header_parity.rs` asserts the installed values.

/// Encode using `\ddd` octal.
pub const VIS_OCTAL: u32 = 0x0001;
/// Encode using C-style escapes (`\n`, `\t`, ...).
pub const VIS_CSTYLE: u32 = 0x0002;
/// Do not encode space.
pub const VIS_SP: u32 = 0x0004;
/// Do not encode tab.
pub const VIS_TAB: u32 = 0x0008;
/// Do not encode newline.
pub const VIS_NL: u32 = 0x0010;
/// `VIS_SP | VIS_TAB | VIS_NL`.
pub const VIS_WHITE: u32 = VIS_SP | VIS_TAB | VIS_NL;
/// Encode only unsafe characters (`\b`, bell, `\r` pass through).
pub const VIS_SAFE: u32 = 0x0020;
/// Encode with `M-x`/`^x` forms only (no leading `\`).
pub const VIS_NOSLASH: u32 = 0x0040;
/// HTTP 1808 style (`%xx`); alias of [`VIS_HTTPSTYLE`].
pub const VIS_HTTP1808: u32 = 0x0080;
/// HTTP style (`%xx`).
pub const VIS_HTTPSTYLE: u32 = 0x0080;
/// Quoted-printable MIME style (`=xx`).
pub const VIS_MIMESTYLE: u32 = 0x0100;
/// RFC 1866 style (`&#N;`/`&name;`).
pub const VIS_HTTP1866: u32 = 0x0200;
/// Do not treat `\` as an escape on input.
pub const VIS_NOESCAPE: u32 = 0x0400;
/// `UNVIS_END` marker (also `_VIS_END` in the header).
pub const VIS_END: u32 = 0x0800;
/// Also encode the glob metacharacters (`*?[#`).
pub const VIS_GLOB: u32 = 0x1000;
/// Also encode the shell metacharacters.
pub const VIS_SHELL: u32 = 0x2000;
/// `VIS_WHITE | VIS_GLOB | VIS_SHELL`.
pub const VIS_META: u32 = VIS_WHITE | VIS_GLOB | VIS_SHELL;
/// Byte-oriented classification (C locale graph instead of `iswgraph`).
pub const VIS_NOLOCALE: u32 = 0x4000;
/// Also encode `"`.
pub const VIS_DQ: u32 = 0x8000;

/// `unvis`: a valid character.
pub const UNVIS_VALID: i32 = 1;
/// `unvis`: a valid character; push the current input byte back.
pub const UNVIS_VALIDPUSH: i32 = 2;
/// `unvis`: no character produced.
pub const UNVIS_NOCHAR: i32 = 3;
/// `unvis`: bad sequence.
pub const UNVIS_SYNBAD: i32 = -1;
/// `unvis`: allocation error.
pub const UNVIS_ERROR: i32 = -2;
/// `unvis`: flush the state machine (`unvis(..., UNVIS_END)`).
pub const UNVIS_END: u32 = VIS_END;
