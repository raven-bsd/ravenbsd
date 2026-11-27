// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Termios glue over the tty `CALL` protocol (`PG-54`).
//!
//! The pty server carries the complete terminal attribute record in `GET_TERMIOS`/`SET_TERMIOS`
//! (K444); `tcgetattr`/`tcsetattr` round-trip every field. Older tty servers that only answer
//! `GET_MODE`/`SET_MODE` (the console scheme) fall back to the canonical/raw plus echo view and
//! the documented defaults. `tcdrain`, flow control, break and window size still return
//! `ENOSYS` until the protocol grows those operations.

use raven_abi::errno::Errno;
use raven_libc_abi::structs::Termios;
pub use raven_proto_tty::Winsize;
use raven_proto_tty::{
    self, Request, Response, Settings, TERMIOS_LEN, Termios as WireTermios, WINSIZE_LEN,
};

use crate::backend::SysBackend;
use crate::fd::ofd::OfdKind;
use crate::lock::Lock;
use crate::platform::Platform;

/// `ICANON` from the public FreeBSD `<bits/termios.h>`.
pub const ICANON: u32 = 0x0000_0100;
/// `ECHO` from the public FreeBSD `<bits/termios.h>`.
pub const ECHO: u32 = 0x0000_0008;
/// `CS8` from the public FreeBSD `<bits/termios.h>`.
pub const CS8: u32 = 0x0000_0300;
/// `CREAD` from the public FreeBSD `<bits/termios.h>`.
pub const CREAD: u32 = 0x0000_0800;
/// Index of `VMIN` in the public `c_cc`.
pub const VMIN: usize = 16;
/// Index of `VTIME` in the public `c_cc`.
pub const VTIME: usize = 17;
/// Index of `VINTR` in the public `c_cc`.
pub const VINTR: usize = 8;
/// Index of `VEOF` in the public `c_cc`.
pub const VEOF: usize = 0;
/// `B38400`, the speed the current console reports.
pub const B38400: u32 = 38_400;

/// `pty` server custom `SET_FG` op: store the foreground process group
/// (`userland/servers/ptyd/src/main.rs:35`).
pub const OP_SET_FG: u16 = 0x10;
/// `pty` server custom `GET_FG` op: read the foreground process group
/// (`userland/servers/ptyd/src/main.rs:36`).
pub const OP_GET_FG: u16 = 0x11;

/// A termios record for a canonical, echoing terminal.
pub fn default_termios() -> Termios {
    termios_from_wire(WireTermios::sane())
}

fn tty_call<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    request: Request,
) -> Result<Response, Errno> {
    let reference = platform.get(fd)?;
    // A v1 spawn entry has no kind; the tty server answers the request for a terminal and a
    // non-tty handle fails it, so an Unknown-kind descriptor is probed instead of rejected.
    if !matches!(reference.kind(), OfdKind::Tty | OfdKind::Unknown) {
        return Err(Errno::ENOTTY);
    }
    let mut body = [0u8; raven_proto_tty::CALL_LEN];
    raven_proto_tty::encode_request(request, &mut body);
    let mut response = [0u8; raven_proto_tty::CALL_LEN];
    let count = match platform
        .backend()
        .call(reference.handle(), &body, &mut response)
    {
        Err(Errno::ENOSYS) => return Err(Errno::ENOTTY),
        other => other?,
    };
    if count < raven_proto_tty::CALL_LEN {
        return Err(Errno::EPROTO);
    }
    let decoded = raven_proto_tty::decode_response(response.get(..count).unwrap_or(&[]))
        .map_err(|_| Errno::EPROTO)?;
    Ok(decoded)
}

fn tty_get_termios<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<WireTermios, Errno> {
    let reference = platform.get(fd)?;
    // A v1 spawn entry has no kind; the tty server answers the request for a terminal and a
    // non-tty handle fails it, so an Unknown-kind descriptor is probed instead of rejected.
    if !matches!(reference.kind(), OfdKind::Tty | OfdKind::Unknown) {
        return Err(Errno::ENOTTY);
    }
    let mut body = [0u8; TERMIOS_LEN];
    raven_proto_tty::encode_get_termios_request(&mut body);
    let mut response = [0u8; TERMIOS_LEN];
    let count = match platform
        .backend()
        .call(reference.handle(), &body, &mut response)
    {
        Err(Errno::ENOSYS) => return Err(Errno::ENOTTY),
        other => other?,
    };
    if count < TERMIOS_LEN {
        return Err(Errno::EPROTO);
    }
    raven_proto_tty::decode_termios_response(response.get(..count).unwrap_or(&[]))
        .map_err(|_| Errno::EPROTO)
}

/// Whether `fd` is a pty endpoint the K444 termios service answers for.
///
/// The console servers implement only `GET_MODE` and answer `GET_TERMIOS` with `ENOSYS`; the pty
/// server answers the full record. This separates a pty from a console without a guess.
pub(crate) fn is_pty_endpoint<B: SysBackend, L: Lock>(platform: &Platform<B, L>, fd: i32) -> bool {
    tty_get_termios(platform, fd).is_ok()
}

fn tty_set_termios<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    termios: &WireTermios,
) -> Result<WireTermios, Errno> {
    let reference = platform.get(fd)?;
    // A v1 spawn entry has no kind; the tty server answers the request for a terminal and a
    // non-tty handle fails it, so an Unknown-kind descriptor is probed instead of rejected.
    if !matches!(reference.kind(), OfdKind::Tty | OfdKind::Unknown) {
        return Err(Errno::ENOTTY);
    }
    let mut body = [0u8; TERMIOS_LEN];
    raven_proto_tty::encode_set_termios_request(termios, &mut body);
    let mut response = [0u8; TERMIOS_LEN];
    let count = match platform
        .backend()
        .call(reference.handle(), &body, &mut response)
    {
        Err(Errno::ENOSYS) => return Err(Errno::ENOTTY),
        other => other?,
    };
    if count < TERMIOS_LEN {
        return Err(Errno::EPROTO);
    }
    raven_proto_tty::decode_termios_response(response.get(..count).unwrap_or(&[]))
        .map_err(|_| Errno::EPROTO)
}

/// `tcgetattr`.
pub fn tcgetattr<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<Termios, Errno> {
    match tty_get_termios(platform, fd) {
        Ok(termios) => Ok(termios_from_wire(termios)),
        Err(Errno::ENOTTY) => match tty_call(platform, fd, Request::GetMode)? {
            Response::Mode(settings) => Ok(termios_from_settings(settings)),
            _ => Err(Errno::EPROTO),
        },
        Err(error) => Err(error),
    }
}

/// `tcsetattr`.
pub fn tcsetattr<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    termios: &Termios,
) -> Result<(), Errno> {
    let wire = wire_from_termios(termios);
    match tty_set_termios(platform, fd, &wire) {
        Ok(_) => Ok(()),
        Err(Errno::ENOTTY) => {
            let settings = settings_from(termios);
            match tty_call(platform, fd, Request::SetMode { settings })? {
                Response::Mode(_) => Ok(()),
                _ => Err(Errno::EPROTO),
            }
        }
        Err(error) => Err(error),
    }
}

/// `tcflush`: the tty protocol discards input and reports the byte count.
pub fn tcflush<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    _queue: i32,
) -> Result<(), Errno> {
    match tty_call(platform, fd, Request::Flush)? {
        Response::Flushed { .. } => Ok(()),
        _ => Err(Errno::EPROTO),
    }
}

/// `tcdrain`: `ENOSYS` until PG-54.
pub fn tcdrain<B: SysBackend, L: Lock>(platform: &Platform<B, L>, fd: i32) -> Result<(), Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

/// `tcflow`: `ENOSYS` until PG-54.
pub fn tcflow<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    _action: i32,
) -> Result<(), Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

/// `tcsendbreak`: `ENOSYS` until PG-54.
pub fn tcsendbreak<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    _duration: i32,
) -> Result<(), Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

/// `cfmakeraw`.
pub fn cfmakeraw(termios: &mut Termios) {
    termios.c_iflag = 0;
    termios.c_oflag = 0;
    termios.c_lflag = 0;
    termios.c_cflag &= !CS8;
    termios.c_cflag |= CS8;
    if let Some(slot) = termios.c_cc.get_mut(VMIN) {
        *slot = 1;
    }
    if let Some(slot) = termios.c_cc.get_mut(VTIME) {
        *slot = 0;
    }
}

/// `cfgetispeed`.
pub const fn cfgetispeed(termios: &Termios) -> u32 {
    termios.c_ispeed
}

/// `cfgetospeed`.
pub const fn cfgetospeed(termios: &Termios) -> u32 {
    termios.c_ospeed
}

/// Whether `speed` is one of the FreeBSD numeric `B*` values.
pub const fn is_supported_speed(speed: u32) -> bool {
    matches!(
        speed,
        0 | 50
            | 75
            | 110
            | 134
            | 150
            | 200
            | 300
            | 600
            | 1200
            | 1800
            | 2400
            | 4800
            | 7200
            | 9600
            | 14_400
            | 19_200
            | 28_800
            | 38_400
            | 57_600
            | 76_800
            | 115_200
            | 230_400
            | 460_800
            | 500_000
            | 921_600
            | 1_000_000
            | 1_500_000
            | 2_000_000
            | 2_500_000
            | 3_000_000
            | 3_500_000
            | 4_000_000
    )
}

/// `cfsetispeed`.
pub fn cfsetispeed(termios: &mut Termios, speed: u32) -> i32 {
    if !is_supported_speed(speed) {
        return -1;
    }
    termios.c_ispeed = speed;
    0
}

/// `cfsetospeed`.
pub fn cfsetospeed(termios: &mut Termios, speed: u32) -> i32 {
    if !is_supported_speed(speed) {
        return -1;
    }
    termios.c_ospeed = speed;
    0
}

/// `tcgetwinsize`: the pty window size, `ENOTTY` for a non-tty or a tty server without the call.
pub fn tcgetwinsize<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<Winsize, Errno> {
    let reference = platform.get(fd)?;
    // A v1 spawn entry has no kind; the tty server answers the request for a terminal and a
    // non-tty handle fails it, so an Unknown-kind descriptor is probed instead of rejected.
    if !matches!(reference.kind(), OfdKind::Tty | OfdKind::Unknown) {
        return Err(Errno::ENOTTY);
    }
    let mut body = [0u8; WINSIZE_LEN];
    raven_proto_tty::encode_get_winsize_request(&mut body);
    let mut response = [0u8; WINSIZE_LEN];
    let count = match platform
        .backend()
        .call(reference.handle(), &body, &mut response)
    {
        Err(Errno::ENOSYS) => return Err(Errno::ENOTTY),
        other => other?,
    };
    if count < WINSIZE_LEN {
        return Err(Errno::EPROTO);
    }
    raven_proto_tty::decode_winsize_response(response.get(..count).unwrap_or(&[]))
        .map_err(|_| Errno::EPROTO)
}

/// `tcsetwinsize`: store the pty window size, `ENOTTY` for a non-tty or a tty server without the
/// call.
pub fn tcsetwinsize<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    winsize: &Winsize,
) -> Result<(), Errno> {
    let reference = platform.get(fd)?;
    // A v1 spawn entry has no kind; the tty server answers the request for a terminal and a
    // non-tty handle fails it, so an Unknown-kind descriptor is probed instead of rejected.
    if !matches!(reference.kind(), OfdKind::Tty | OfdKind::Unknown) {
        return Err(Errno::ENOTTY);
    }
    let mut body = [0u8; WINSIZE_LEN];
    raven_proto_tty::encode_set_winsize_request(winsize, &mut body);
    let mut response = [0u8; WINSIZE_LEN];
    let count = match platform
        .backend()
        .call(reference.handle(), &body, &mut response)
    {
        Err(Errno::ENOSYS) => return Err(Errno::ENOTTY),
        other => other?,
    };
    if count < WINSIZE_LEN {
        return Err(Errno::EPROTO);
    }
    raven_proto_tty::decode_winsize_response(response.get(..count).unwrap_or(&[]))
        .map(|_| ())
        .map_err(|_| Errno::EPROTO)
}

fn custom_body(op: u16, arg0: u32) -> [u8; raven_proto_tty::CALL_LEN] {
    let mut body = [0u8; raven_proto_tty::CALL_LEN];
    body[0..2].copy_from_slice(&raven_proto_tty::VERSION.to_le_bytes());
    body[2..4].copy_from_slice(&op.to_le_bytes());
    body[4..8].copy_from_slice(&arg0.to_le_bytes());
    body
}

fn custom_arg0(response: &[u8], expected_op: u16) -> Result<u32, Errno> {
    if response.len() < raven_proto_tty::CALL_LEN {
        return Err(Errno::EPROTO);
    }
    let version = u16::from_le_bytes(
        response
            .get(0..2)
            .ok_or(Errno::EPROTO)?
            .try_into()
            .map_err(|_| Errno::EPROTO)?,
    );
    if version != raven_proto_tty::VERSION {
        return Err(Errno::EPROTO);
    }
    let op = u16::from_le_bytes(
        response
            .get(2..4)
            .ok_or(Errno::EPROTO)?
            .try_into()
            .map_err(|_| Errno::EPROTO)?,
    );
    if op != expected_op {
        return Err(Errno::EPROTO);
    }
    Ok(u32::from_le_bytes(
        response
            .get(4..8)
            .ok_or(Errno::EPROTO)?
            .try_into()
            .map_err(|_| Errno::EPROTO)?,
    ))
}

fn tty_custom<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    op: u16,
    arg0: u32,
) -> Result<u32, Errno> {
    let reference = platform.get(fd)?;
    // A v1 spawn entry has no kind; the tty server answers the request for a terminal and a
    // non-tty handle fails it, so an Unknown-kind descriptor is probed instead of rejected.
    if !matches!(reference.kind(), OfdKind::Tty | OfdKind::Unknown) {
        return Err(Errno::ENOTTY);
    }
    let body = custom_body(op, arg0);
    let mut response = [0u8; raven_proto_tty::CALL_LEN];
    let count = match platform
        .backend()
        .call(reference.handle(), &body, &mut response)
    {
        Err(Errno::ENOSYS) => return Err(Errno::ENOTTY),
        other => other?,
    };
    custom_arg0(response.get(..count).unwrap_or(&[]), op)
}

/// `tcgetpgrp`: the terminal's foreground process group, or `ENOTTY` for a non-tty.
pub fn tcgetpgrp<B: SysBackend, L: Lock>(platform: &Platform<B, L>, fd: i32) -> Result<i32, Errno> {
    let value = tty_custom(platform, fd, OP_GET_FG, 0)?;
    i32::try_from(value).map_err(|_| Errno::EOVERFLOW)
}

/// `tcsetpgrp`: set the terminal's foreground process group.
///
/// The caller-side POSIX session check is in the shell (`raven-libc`); this platform function
/// rejects a non-positive group and forwards the existing ptyd `SET_FG` op.
pub fn tcsetpgrp<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    pgrp: i32,
) -> Result<(), Errno> {
    if pgrp <= 0 {
        return Err(Errno::EINVAL);
    }
    tty_custom(platform, fd, OP_SET_FG, pgrp as u32).map(|_| ())
}

fn termios_from_settings(settings: Settings) -> Termios {
    let mut termios = default_termios();
    if settings.mode == raven_proto_tty::Mode::Raw {
        termios.c_lflag = 0;
        termios.c_iflag = 0;
        termios.c_oflag = 0;
    } else {
        termios.c_lflag |= ICANON;
    }
    if settings.echo {
        termios.c_lflag |= ECHO;
    } else {
        termios.c_lflag &= !ECHO;
    }
    termios
}

fn settings_from(termios: &Termios) -> Settings {
    Settings {
        mode: if termios.c_lflag & ICANON != 0 {
            raven_proto_tty::Mode::Canonical
        } else {
            raven_proto_tty::Mode::Raw
        },
        echo: termios.c_lflag & ECHO != 0,
    }
}

/// Public `c_cc` index -> private tty-wire `c_cc` index, for all 20 public slots.
///
/// `VERASE2` (7), `VDSUSP` (11), `VSTATUS` (18) and the spare (19) have no tty-core consumer;
/// they are stored in a dedicated wire slot and returned unchanged, with no behavioural effect.
const CC_MAP: &[(usize, usize)] = &[
    (0, 4),   // VEOF
    (1, 11),  // VEOL
    (2, 16),  // VEOL2
    (3, 2),   // VERASE
    (4, 14),  // VWERASE
    (5, 3),   // VKILL
    (6, 12),  // VREPRINT
    (7, 7),   // VERASE2 (wire VSWTC slot, unused)
    (8, 0),   // VINTR
    (9, 1),   // VQUIT
    (10, 10), // VSUSP
    (11, 17), // VDSUSP (wire spare, unused)
    (12, 8),  // VSTART
    (13, 9),  // VSTOP
    (14, 15), // VLNEXT
    (15, 13), // VDISCARD
    (16, 6),  // VMIN
    (17, 5),  // VTIME
    (18, 18), // VSTATUS (wire spare, unused)
    (19, 19), // spare 19
];

/// FreeBSD numeric speed -> private tty-wire speed encoding.
///
/// The private wire now carries the donor numeric speeds too, so the map is the explicit
/// bidirectional identity list. `B7200`, `B14400`, `B28800` and `B76800` have no Linux/musl
/// code; they use the numeric speed itself as the documented private value.
pub const SPEED_MAP: &[(u32, u32)] = &[
    (0, 0),
    (50, 50),
    (75, 75),
    (110, 110),
    (134, 134),
    (150, 150),
    (200, 200),
    (300, 300),
    (600, 600),
    (1200, 1200),
    (1800, 1800),
    (2400, 2400),
    (4800, 4800),
    (7200, 7200),
    (9600, 9600),
    (14_400, 14_400),
    (19_200, 19_200),
    (28_800, 28_800),
    (38_400, 38_400),
    (57_600, 57_600),
    (76_800, 76_800),
    (115_200, 115_200),
    (230_400, 230_400),
    (460_800, 460_800),
    (500_000, 500_000),
    (921_600, 921_600),
    (1_000_000, 1_000_000),
    (1_500_000, 1_500_000),
    (2_000_000, 2_000_000),
    (2_500_000, 2_500_000),
    (3_000_000, 3_000_000),
    (3_500_000, 3_500_000),
    (4_000_000, 4_000_000),
];

fn speed_to_wire(speed: u32) -> u32 {
    SPEED_MAP
        .iter()
        .find(|(public, _)| *public == speed)
        .map_or(0, |(_, private)| *private)
}

fn speed_from_wire(speed: u32) -> u32 {
    SPEED_MAP
        .iter()
        .find(|(_, private)| *private == speed)
        .map_or(0, |(public, _)| *public)
}

fn cc_to_wire(cc: &[u8; 20]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for &(public, private) in CC_MAP {
        out[private] = cc[public];
    }
    out
}

fn cc_from_wire(cc: &[u8; 32]) -> [u8; 20] {
    let mut out = [0u8; 20];
    for &(public, private) in CC_MAP {
        out[public] = cc[private];
    }
    out
}

fn termios_from_wire(wire: WireTermios) -> Termios {
    let mut termios = Termios::ZERO;
    // The public and wire records now use the same donor flag words and numeric speeds; every
    // bit is preserved, including bits tty-core does not act on.
    termios.c_iflag = wire.iflag;
    termios.c_oflag = wire.oflag;
    termios.c_cflag = wire.cflag;
    termios.c_lflag = wire.lflag;
    termios.c_cc = cc_from_wire(&wire.cc);
    termios.c_ispeed = speed_from_wire(wire.ispeed);
    termios.c_ospeed = speed_from_wire(wire.ospeed);
    termios
}

fn wire_from_termios(termios: &Termios) -> WireTermios {
    WireTermios {
        iflag: termios.c_iflag,
        oflag: termios.c_oflag,
        cflag: termios.c_cflag,
        lflag: termios.c_lflag,
        line: 0,
        cc: cc_to_wire(&termios.c_cc),
        ispeed: speed_to_wire(termios.c_ispeed),
        ospeed: speed_to_wire(termios.c_ospeed),
    }
}

#[cfg(test)]
#[path = "termios_ut.rs"]
mod tests;
