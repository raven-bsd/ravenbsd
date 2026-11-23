// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Controlling terminal, foreground process group and background-access policy.
//!
//! A process acquires a tty handle with `PROC_CTTY_SET` (the native `TIOCSCTTY` operation). The
//! kernel records the tty's server/file pair, its session id and the foreground process group.
//! Reads and writes on that tty are checked before the scheme request reaches the server:
//! a background group gets `SIGTTIN`/`SIGTTOU`, an orphaned group gets `EIO`, and a hung tty gets
//! `EIO`. The tty server still owns the data path; this module only owns the POSIX control state.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use spin::Mutex;

use crate::cmdline::CmdLine;
use crate::kprintln;
use crate::memory::paging;

/// K495 rev180 diagnostic gate: the pty gate image boots with `k495-debug=1`.
fn debug_enabled() -> bool {
    CmdLine::parse(paging::cmdline()).get("k495-debug").is_some()
}

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::ipc::SchemeFile;
#[cfg(target_arch = "x86_64")]
use crate::ipc::SchemeFile;

use super::{Process, group_in_session, group_orphaned, signal_group, status};

/// One controlling-terminal state, shared by every process in its session.
pub struct Ctty {
    key: (u64, u64),
    /// The terminal open-file description. `/dev/tty` opens this live object; FreeBSD keeps the
    /// controlling vnode alive for the session in the same way.
    file: Arc<SchemeFile>,
    fg_pgrp: AtomicI64,
    sid: AtomicU64,
    hung: AtomicBool,
}

impl Ctty {
    /// The session that owns this terminal.
    pub fn sid(&self) -> u64 {
        self.sid.load(Ordering::Acquire)
    }

    /// The terminal open-file description for `/dev/tty`.
    pub fn file(&self) -> &Arc<SchemeFile> {
        &self.file
    }

    /// The foreground process group, or zero when unset.
    pub fn fg_pgrp(&self) -> i64 {
        self.fg_pgrp.load(Ordering::Acquire)
    }

    /// True after hangup.
    pub fn is_hung(&self) -> bool {
        self.hung.load(Ordering::Acquire)
    }
}

static REGISTRY: Mutex<alloc::vec::Vec<alloc::sync::Weak<Ctty>>> =
    Mutex::new(alloc::vec::Vec::new());

/// True when `file` belongs to a terminal scheme (`ttyd` or the console `tty`).
pub fn is_tty_file(file: &SchemeFile) -> bool {
    file.server
        .schemes
        .lock()
        .iter()
        .any(|scheme| scheme.name == "ttyd" || scheme.name == "tty" || scheme.name == "pty")
}

fn find(file: &SchemeFile) -> Option<Arc<Ctty>> {
    let key = (file.server.id, file.server_id());
    let registry = REGISTRY.lock();
    for weak in registry.iter() {
        if let Some(ctty) = weak.upgrade()
            && ctty.key == key
        {
            return Some(ctty);
        }
    }
    None
}

/// The live controlling terminal of session `sid`, if any.
///
/// A process created by `posix_spawn` starts without its own `ctty` slot but stays in the
/// parent's session; `/dev/tty` resolves to the session's terminal (FreeBSD `cttyopen` looks up
/// the session's controlling vnode, not a per-process copy).
pub fn session(sid: u64) -> Option<Arc<Ctty>> {
    if sid == 0 {
        return None;
    }
    let debug = debug_enabled();
    let registry = REGISTRY.lock();
    for weak in registry.iter() {
        let Some(ctty) = weak.upgrade() else {
            continue;
        };
        if ctty.sid() == sid {
            return Some(ctty);
        }
        if debug {
            kprintln!(
                "ctty: registry entry sid={} wanted={}",
                ctty.sid(),
                sid
            );
        }
    }
    if debug {
        kprintln!("ctty: registry no entry wanted={}", sid);
    }
    None
}

fn get_or_create(file: &Arc<SchemeFile>) -> Arc<Ctty> {
    if let Some(ctty) = find(file) {
        return ctty;
    }
    let ctty = Arc::new(Ctty {
        key: (file.server.id, file.server_id()),
        file: Arc::clone(file),
        fg_pgrp: AtomicI64::new(0),
        sid: AtomicU64::new(0),
        hung: AtomicBool::new(false),
    });
    let mut registry = REGISTRY.lock();
    registry.retain(|weak| weak.strong_count() > 0);
    registry.push(Arc::downgrade(&ctty));
    ctty
}

/// `TIOCSCTTY`: acquire `file` as the calling session leader's controlling terminal.
pub fn acquire(process: &Arc<Process>, file: &Arc<SchemeFile>) -> Result<(), Errno> {
    let debug = debug_enabled();
    let ids = *process.ids.lock();
    if debug {
        kprintln!(
            "ctty: acquire pid={} sid={} pgid={} tty={} server={} file={}",
            process.id,
            ids.sid,
            ids.pgid,
            is_tty_file(file),
            file.server.id,
            file.server_id()
        );
    }
    if !is_tty_file(file) {
        if debug {
            kprintln!("ctty: acquire pid={} rejected ENOTTY (not a tty file)", process.id);
        }
        return Err(Errno::ENOTTY);
    }
    if ids.sid != process.id {
        if debug {
            kprintln!(
                "ctty: acquire pid={} rejected EPERM (sid={} is not the pid)",
                process.id,
                ids.sid
            );
        }
        return Err(Errno::EPERM);
    }
    let mut slot = process.ctty.lock();
    if let Some(existing) = slot.as_ref() {
        if existing.key == (file.server.id, file.server_id()) && existing.sid() == ids.sid {
            if debug {
                kprintln!(
                    "ctty: acquire pid={} already owns sid={} ctty_sid={}",
                    process.id,
                    ids.sid,
                    existing.sid()
                );
            }
            return Ok(());
        }
        if debug {
            kprintln!(
                "ctty: acquire pid={} rejected EPERM (existing ctty sid={} wanted={})",
                process.id,
                existing.sid(),
                ids.sid
            );
        }
        return Err(Errno::EPERM);
    }
    let ctty = get_or_create(file);
    if ctty.sid() != 0 && ctty.sid() != ids.sid {
        if debug {
            kprintln!(
                "ctty: acquire pid={} rejected EBUSY (ctty sid={} wanted={})",
                process.id,
                ctty.sid(),
                ids.sid
            );
        }
        return Err(Errno::EBUSY);
    }
    ctty.sid.store(ids.sid, Ordering::Release);
    ctty.fg_pgrp.store(ids.pgid as i64, Ordering::Release);
    ctty.hung.store(false, Ordering::Release);
    *slot = Some(Arc::clone(&ctty));
    if debug {
        kprintln!("ctty: acquire pid={} ok sid={}", process.id, ids.sid);
    }
    Ok(())
}

/// Drop the process's controlling-terminal reference (used by `setsid`).
pub fn clear_session(process: &Process) {
    let _ = process.ctty.lock().take();
}

/// `tcgetpgrp`: foreground process group for `file`.
pub fn get_fg(process: &Process, file: &SchemeFile) -> Result<i32, Errno> {
    let ctty = find(file).ok_or(Errno::ENOTTY)?;
    let ids = *process.ids.lock();
    if ids.euid != 0 && ids.sid != ctty.sid() {
        return Err(Errno::ENOTTY);
    }
    i32::try_from(ctty.fg_pgrp()).map_err(|_| Errno::EINVAL)
}

/// `tcsetpgrp`: set the foreground process group for `file`.
pub fn set_fg(process: &Process, file: &SchemeFile, pgrp: i32) -> Result<(), Errno> {
    let ctty = find(file).ok_or(Errno::ENOTTY)?;
    let ids = *process.ids.lock();
    if ids.sid != ctty.sid() {
        return Err(Errno::ENOTTY);
    }
    let pgrp = if pgrp == 0 {
        ids.pgid
    } else {
        u64::try_from(pgrp).map_err(|_| Errno::EINVAL)?
    };
    if pgrp == 0 || !group_in_session(pgrp, ids.sid) {
        return Err(Errno::ESRCH);
    }
    let fg = ctty.fg_pgrp();
    if ids.euid != 0 && ids.sid != process.id && fg != ids.pgid as i64 {
        return Err(Errno::EPERM);
    }
    ctty.fg_pgrp.store(pgrp as i64, Ordering::Release);
    Ok(())
}

/// Check a read/write against the controlling-terminal policy.
///
/// Returns `Ok(())` when the access may proceed. A background group gets the matching stop
/// signal and `EINTR`; an orphaned group and a hung terminal get `EIO`.
pub fn check_access(
    process: &Arc<Process>,
    file: &Arc<SchemeFile>,
    write: bool,
) -> Result<(), Errno> {
    let Some(ctty) = find(file) else {
        return Ok(());
    };
    let ids = *process.ids.lock();
    if ctty.sid() == 0 || ids.sid != ctty.sid() {
        return Ok(());
    }
    if ctty.is_hung() {
        return Err(Errno::EIO);
    }
    let fg = ctty.fg_pgrp();
    if fg == 0 || fg == ids.pgid as i64 {
        return Ok(());
    }
    if group_orphaned(ids.pgid, ids.sid) {
        return Err(Errno::EIO);
    }
    let signal = if write {
        status::SIGTTOU
    } else {
        status::SIGTTIN
    };
    let _ = signal_group(ids.pgid, signal, 0);
    Err(Errno::EINTR)
}

/// Hang up the controlling terminal when its session leader exits.
pub fn hangup_session_leader(process: &Arc<Process>) {
    let ids = *process.ids.lock();
    if ids.sid != process.id {
        return;
    }
    let ctty = process.ctty.lock().take();
    let Some(ctty) = ctty else {
        return;
    };
    if ctty.sid() != ids.sid {
        return;
    }
    ctty.hung.store(true, Ordering::Release);
    let fg = ctty.fg_pgrp();
    if fg > 0 {
        let pgid = u64::try_from(fg).unwrap_or(0);
        if pgid != 0 {
            let _ = signal_group(pgid, status::SIGHUP, 0);
            let _ = signal_group(pgid, status::SIGCONT, 0);
        }
    }
}
