// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `Platform`: the object that owns the fd table and performs the POSIX operations.
//!
//! The shell creates one `Platform<RealSys, ShellLock>` and publishes it as a process global;
//! tests create one `Platform<FakeSys>` per test. All table mutation happens under the
//! [`Lock`] seam and inside a short `RefCell` borrow; no borrow and no lock is ever held across a
//! backend call.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_libc_abi::fcntl;
use raven_libc_abi::structs::Stat;
use raven_proto_fs::{Call, CallRequest, CallResponse, SetTime, Stat as WireStat};

use crate::backend::SysBackend;
use crate::fd::fdsan::{Action, FdsanState, Mismatch};
use crate::fd::inherit::{ENV_NAME, FdMap, FdMapEntry, InheritedSocket, transferable};
use crate::fd::ofd::{OfdArena, OfdKind, OfdRef, SocketMeta};
use crate::fd::table::{FD_CLOEXEC, FdTable};
use crate::fd::{DEFAULT_HARD_NOFILE, DEFAULT_SOFT_NOFILE};
use crate::lock::{Lock, NoLock, guard};
use crate::path::cwd::Cwd;
use crate::path::resolve::{DEFAULT_LIMITS, Limits, file_path, resolve};
use crate::path::special::{special_fd, target_fd};
use crate::stat::StatSource;
use crate::xlat;

/// Process resource limits kept in libc (`RLIMIT_NOFILE` for the fd table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rlimits {
    /// Soft `RLIMIT_NOFILE`.
    pub nofile_soft: u32,
    /// Hard `RLIMIT_NOFILE`.
    pub nofile_hard: u32,
}

impl Default for Rlimits {
    fn default() -> Self {
        Self {
            nofile_soft: DEFAULT_SOFT_NOFILE,
            nofile_hard: DEFAULT_HARD_NOFILE,
        }
    }
}

/// The epoll registration table: key `(epfd, cookie)`, value `(event flags, data)`.
pub(crate) type EpollState = alloc::collections::BTreeMap<(i32, u64), (u32, u64)>;

/// The platform object.
pub struct Platform<B: SysBackend, L: Lock = NoLock> {
    backend: B,
    lock: L,
    fd: RefCell<FdTable>,
    ofd: RefCell<OfdArena>,
    cwd: Cwd,
    fdsan: FdsanState,
    root: RefCell<Option<Handle>>,
    event_queue: RefCell<Option<Handle>>,
    rand: RefCell<Option<Handle>>,
    epoll: RefCell<EpollState>,
    umask: Cell<u32>,
    rlimits: Cell<Rlimits>,
    limits: Limits,
}

impl<B: SysBackend> Platform<B, NoLock> {
    /// A single-threaded platform (tests, early start before the shell lock exists).
    pub fn new(backend: B) -> Self {
        Self::with_lock(backend, NoLock)
    }
}

impl<B: SysBackend, L: Lock> Platform<B, L> {
    /// A platform with the shell-provided lock.
    pub fn with_lock(backend: B, lock: L) -> Self {
        let rlimits = Rlimits::default();
        Self {
            backend,
            lock,
            fd: RefCell::new(FdTable::new(rlimits.nofile_soft, rlimits.nofile_hard)),
            ofd: RefCell::new(OfdArena::new()),
            cwd: Cwd::new(),
            fdsan: FdsanState::default(),
            root: RefCell::new(None),
            event_queue: RefCell::new(None),
            rand: RefCell::new(None),
            epoll: RefCell::new(alloc::collections::BTreeMap::new()),
            umask: Cell::new(0o022),
            rlimits: Cell::new(rlimits),
            limits: DEFAULT_LIMITS,
        }
    }

    /// Override the path limits (tests use a 4096 transport limit to exercise long paths).
    pub fn set_path_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    /// The background backend.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// The fdsan state.
    pub fn fdsan(&self) -> &FdsanState {
        &self.fdsan
    }

    /// The lock seam.
    pub fn lock(&self) -> &L {
        &self.lock
    }

    /// The current cwd.
    pub fn cwd(&self) -> Vec<u8> {
        self.cwd.get()
    }

    /// Replace the cwd directly (after a successful `chdir` verification).
    pub fn set_cwd(&self, path: &[u8]) {
        self.cwd.set(path);
    }

    /// Initialise the cwd from `PWD` when it is absolute and resolvable, else keep `/`.
    pub fn init_cwd(&self, pwd: Option<&[u8]>) {
        if let Some(candidate) = pwd
            && candidate.starts_with(b"/")
            && let Ok(resolved) = resolve(b"/", candidate, false, &self.limits)
        {
            self.cwd.set(&resolved.abs);
        }
    }

    /// Adopt a handle received from another process: probe its kind once and cache it.
    pub fn adopt(
        &self,
        handle: Handle,
        status: u32,
        path: Option<Box<[u8]>>,
        cloexec: bool,
    ) -> Result<i32, Errno> {
        let kind = match path.as_deref() {
            Some(path) if is_tty_path(path) => OfdKind::Tty,
            _ => crate::scheme::probe_kind(&self.backend, handle),
        };
        self.install(handle, kind, status, path, cloexec)
    }

    /// Configure the fdsan error level from `RAVEN_FDSAN` (H-9).
    pub fn init_fdsan(&self, value: Option<&[u8]>) {
        self.fdsan
            .set_level(crate::fd::fdsan::ErrorLevel::from_env(value));
    }

    /// `getcwd(buf, size)` with the COPY row 6 argument rules; `None` allocates.
    pub fn getcwd(&self, buf: Option<&mut [u8]>, size: usize) -> Result<Option<Vec<u8>>, Errno> {
        let cwd = self.cwd.get();
        crate::dir::getcwd(&cwd, buf, size)
    }

    /// The process umask.
    pub fn umask(&self) -> u32 {
        self.umask.get()
    }

    /// Set the umask and return the previous value.
    pub fn set_umask(&self, mask: u32) -> u32 {
        self.umask.replace(mask & 0o777)
    }

    /// The rlimit cell.
    pub fn rlimits(&self) -> Rlimits {
        self.rlimits.get()
    }

    /// Change the nofile limits (the table grows lazily).
    pub fn set_nofile_limits(&self, soft: u32, hard: u32) {
        let mut limits = self.rlimits.get();
        limits.nofile_hard = hard.max(crate::fd::table::INLINE_ENTRIES as u32);
        limits.nofile_soft = soft.min(limits.nofile_hard);
        self.rlimits.set(limits);
        let mut table = self.fd.borrow_mut();
        table.set_hard_limit(limits.nofile_hard);
        table.set_soft_limit(limits.nofile_soft);
    }

    // ----------------------------------------------------------------- fd table

    /// A counted reference to an OFD.
    pub fn get(&self, fd: i32) -> Result<OfdRef<'_, B, L>, Errno> {
        let _guard = guard(&self.lock);
        let index = {
            let table = self.fd.borrow();
            let entry = table.entry(fd).ok_or(Errno::EBADF)?;
            if entry.is_free() {
                return Err(Errno::EBADF);
            }
            entry.ofd
        };
        let mut arena = self.ofd.borrow_mut();
        arena.add_ref(index)?;
        drop(arena);
        drop(_guard);
        Ok(OfdRef::new(self, index))
    }

    /// A snapshot of an OFD (used by [`OfdRef`]).
    pub(crate) fn ofd_snapshot(&self, index: u32) -> Option<crate::fd::ofd::Ofd> {
        self.ofd.borrow().get(index).cloned()
    }

    pub(crate) fn release_ofd_ref(&self, index: u32) {
        let close = {
            let _guard = guard(&self.lock);
            self.take_close_handles(index)
        };
        let _ = self.close_handles(close);
    }

    /// Drop one reference and, at zero, take the data and event handles out of the arena. The
    /// caller must close the returned handles **after** releasing the lock.
    fn take_close_handles(&self, index: u32) -> [Handle; 2] {
        let mut arena = self.ofd.borrow_mut();
        if arena.drop_ref(index) {
            arena
                .release(index)
                .map_or([Handle::NONE; 2], |ofd| [ofd.handle.get(), ofd.event.get()])
        } else {
            [Handle::NONE; 2]
        }
    }

    fn close_handle(&self, handle: Handle) -> Result<(), Errno> {
        if handle == Handle::NONE {
            return Ok(());
        }
        match self.backend.close(handle) {
            Err(Errno::EINTR) => Ok(()),
            other => other,
        }
    }

    fn close_handles(&self, handles: [Handle; 2]) -> Result<(), Errno> {
        let [data, event] = handles;
        let mut first = self.close_handle(data);
        // The second handle is a socket's readiness doorbell (a `UserEvent`). `CLOSE` answers
        // `EBADF` for it and leaves it open; only `HANDLE_CLOSE` releases it.
        if event != Handle::NONE {
            let result = match self.backend.handle_close(event) {
                Err(Errno::EINTR) => Ok(()),
                other => other,
            };
            if first.is_ok() && result.is_err() {
                first = result;
            }
        }
        first
    }

    pub(crate) fn set_ofd_status(&self, index: u32, status: u32) -> u32 {
        let mut arena = self.ofd.borrow_mut();
        match arena.get_mut(index) {
            Some(ofd) => ofd
                .status
                .swap(status, core::sync::atomic::Ordering::Relaxed),
            None => 0,
        }
    }

    /// Record the socket identity that an inherited OFD must restore after `execve`.
    pub fn set_ofd_socket_meta(&self, index: u32, meta: SocketMeta) {
        if let Some(ofd) = self.ofd.borrow().get(index) {
            ofd.set_socket_meta(meta);
        }
    }

    /// Every live socket OFD with the identity needed to rebuild the libc socket table.
    ///
    /// Called by the socket layer after startup restored the inheritance block. Descriptors
    /// closed by `close_cloexec_fds` are already absent from the fd table.
    #[must_use]
    pub fn inherited_sockets(&self) -> Vec<InheritedSocket> {
        let entries = self.fd.borrow().open_entries();
        let arena = self.ofd.borrow();
        let mut out = Vec::new();
        for (fd, entry) in entries {
            let Some(ofd) = arena.get(entry.ofd) else {
                continue;
            };
            if ofd.kind != OfdKind::Socket {
                continue;
            }
            let Some(socket) = ofd.socket_meta() else {
                continue;
            };
            out.push(InheritedSocket {
                fd,
                ofd: entry.ofd,
                handle: ofd.handle.get(),
                status: ofd.status.load(core::sync::atomic::Ordering::Relaxed),
                socket,
            });
        }
        out
    }

    pub(crate) fn set_ofd_kind(&self, index: u32, kind: OfdKind) {
        if let Some(ofd) = self.ofd.borrow_mut().get_mut(index) {
            ofd.kind = kind;
        }
    }

    pub(crate) fn set_ofd_dir_cursor(&self, index: u32, ok: bool) {
        if let Some(ofd) = self.ofd.borrow_mut().get_mut(index) {
            ofd.dir_cursor_ok = ok;
        }
    }

    pub(crate) fn set_ofd_pty_master(&self, index: u32) {
        if let Some(ofd) = self.ofd.borrow_mut().get_mut(index) {
            ofd.set_pty_master();
        }
    }

    /// Attach the notification event a scheme-backed socket uses for `poll` readiness.
    pub fn set_ofd_event(&self, index: u32, event: Handle) -> Result<(), Errno> {
        let mut arena = self.ofd.borrow_mut();
        let slot = arena.get_mut(index).ok_or(Errno::EBADF)?;
        slot.set_event(event)
    }

    /// Install a handle and return the new lowest-free fd.
    pub fn install(
        &self,
        handle: Handle,
        kind: OfdKind,
        status: u32,
        path: Option<Box<[u8]>>,
        cloexec: bool,
    ) -> Result<i32, Errno> {
        self.install_impl(None, handle, kind, status, path, cloexec, 0)
    }

    /// Install an OFD that has no kernel handle yet and return the new lowest-free fd.
    ///
    /// `socket(2)` must return a descriptor before `bind`/`connect`/`listen` decides the scheme
    /// endpoint. The lazy-socket seam installs this pending socket OFD and supplies the handle
    /// later with [`Platform::attach_handle`].
    pub fn install_pending(
        &self,
        kind: OfdKind,
        status: u32,
        path: Option<Box<[u8]>>,
        cloexec: bool,
    ) -> Result<i32, Errno> {
        self.install_impl(None, Handle::NONE, kind, status, path, cloexec, 0)
    }

    /// The kernel handle currently cached by an OFD.
    ///
    /// `fork_child_inherit` replaces a socket's parent handle with the child's fresh handle; a
    /// caller holding the old `Sock` value must query the OFD instead of using the stale copy.
    #[must_use]
    pub fn ofd_handle(&self, index: u32) -> Option<Handle> {
        let arena = self.ofd.borrow();
        arena
            .get(index)
            .map(|ofd| ofd.handle.get())
            .filter(|handle| *handle != Handle::NONE)
    }

    /// Attach the kernel handle to an OFD installed with [`Platform::install_pending`].
    ///
    /// The handle is owned by the fd table from this point on. A second attach is `EEXIST`, and
    /// an OFD that already owns a handle is never replaced.
    pub fn attach_handle(&self, ofd: u32, handle: Handle) -> Result<(), Errno> {
        if handle == Handle::NONE {
            return Err(Errno::EINVAL);
        }
        let _guard = guard(&self.lock);
        let mut arena = self.ofd.borrow_mut();
        let slot = arena.get_mut(ofd).ok_or(Errno::EBADF)?;
        slot.set_handle(handle)
    }

    /// Install a handle at exactly `fd`, closing any displaced OFD.
    #[allow(clippy::too_many_arguments)]
    pub fn install_at_fd(
        &self,
        fd: u32,
        handle: Handle,
        kind: OfdKind,
        status: u32,
        path: Option<Box<[u8]>>,
        cloexec: bool,
        tag: u64,
    ) -> Result<(), Errno> {
        self.install_impl(Some(fd), handle, kind, status, path, cloexec, tag)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn install_impl(
        &self,
        fd: Option<u32>,
        handle: Handle,
        kind: OfdKind,
        status: u32,
        path: Option<Box<[u8]>>,
        cloexec: bool,
        tag: u64,
    ) -> Result<i32, Errno> {
        let (installed, close) = {
            let _guard = guard(&self.lock);
            let mut table = self.fd.borrow_mut();
            let installed = match fd {
                Some(fd) => fd,
                None => table.alloc(0)?,
            };
            let mut arena = self.ofd.borrow_mut();
            let index = arena.insert(handle, kind, status, path);
            let mut displaced = None;
            if let Some(entry) = table.entry(installed as i32)
                && !entry.is_free()
            {
                displaced = table.remove(installed as i32);
            }
            if let Err(error) = table.install_at(installed, index, cloexec, tag) {
                let _ = arena.release(index);
                return Err(error);
            }
            let close = displaced.and_then(|entry| {
                if arena.drop_ref(entry.ofd) {
                    arena
                        .release(entry.ofd)
                        .map(|ofd| [ofd.handle.get(), ofd.event.get()])
                } else {
                    None
                }
            });
            (installed, close)
        };
        self.close_handles(close.unwrap_or([Handle::NONE; 2]))?;
        Ok(installed as i32)
    }

    /// `dup`: share the OFD, lowest free fd `>= min_fd`.
    pub fn dup(&self, fd: i32, min_fd: u32, cloexec: bool) -> Result<i32, Errno> {
        let _guard = guard(&self.lock);
        let (index, tag) = {
            let table = self.fd.borrow();
            let entry = table.entry(fd).ok_or(Errno::EBADF)?;
            if entry.is_free() {
                return Err(Errno::EBADF);
            }
            (entry.ofd, entry.tag)
        };
        let new_fd = {
            let mut table = self.fd.borrow_mut();
            let new_fd = table.alloc(min_fd)?;
            table.install_at(new_fd, index, cloexec, tag)?;
            new_fd
        };
        self.ofd.borrow_mut().add_ref(index)?;
        Ok(new_fd as i32)
    }

    /// `dup2`/`dup3`: `old == new` returns `new` after checking `old` is open.
    pub fn dup2(&self, old: i32, new: i32, cloexec: bool) -> Result<i32, Errno> {
        if new < 0 {
            return Err(Errno::EBADF);
        }
        let (index, tag) = {
            let table = self.fd.borrow();
            let entry = table.entry(old).ok_or(Errno::EBADF)?;
            if entry.is_free() {
                return Err(Errno::EBADF);
            }
            (entry.ofd, entry.tag)
        };
        if old == new {
            return Ok(new);
        }
        let (displaced, close) = {
            let _guard = guard(&self.lock);
            if u32::try_from(new).map_or(true, |fd| fd >= self.fd.borrow().hard_limit()) {
                return Err(Errno::EBADF);
            }
            self.ofd.borrow_mut().add_ref(index)?;
            let mut table = self.fd.borrow_mut();
            let mut displaced = None;
            if let Some(entry) = table.entry(new)
                && !entry.is_free()
            {
                displaced = table.remove(new);
            }
            table.install_at(new as u32, index, cloexec, tag)?;
            let close = displaced.map(|entry| self.take_close_handles(entry.ofd));
            (displaced, close)
        };
        let _ = displaced;
        self.close_handles(close.unwrap_or([Handle::NONE; 2]))?;
        Ok(new)
    }

    /// `close`; `EINTR` from the last `CLOSE` is success (A-sys-11).
    pub fn close(&self, fd: i32) -> Result<(), Errno> {
        let close = {
            let _guard = guard(&self.lock);
            let entry = {
                let mut table = self.fd.borrow_mut();
                table.remove(fd).ok_or(Errno::EBADF)?
            };
            self.take_close_handles(entry.ofd)
        };
        self.close_handles(close)
    }

    /// `close` with an expected fdsan tag.
    pub fn close_with_tag(&self, fd: i32, expected: u64) -> Result<(), Errno> {
        let actual = self.fd.borrow().tag(fd);
        if expected != actual {
            let mismatch = if expected != 0 && actual != 0 {
                Mismatch::WrongOwner { expected, actual }
            } else if expected != 0 {
                Mismatch::ExpectedOwned { expected }
            } else {
                Mismatch::ExpectedUnowned { actual }
            };
            let action = self.fdsan.classify(mismatch);
            #[cfg(feature = "fdsan")]
            {
                self.report_fdsan(mismatch, action);
            }
            #[cfg(not(feature = "fdsan"))]
            {
                let _ = action;
            }
        }
        match self.close(fd) {
            Err(Errno::EBADF) if expected != 0 => {
                let mismatch = Mismatch::DoubleClose;
                let action = self.fdsan.classify(mismatch);
                #[cfg(feature = "fdsan")]
                {
                    self.report_fdsan(mismatch, action);
                }
                #[cfg(not(feature = "fdsan"))]
                {
                    let _ = action;
                }
                Err(Errno::EBADF)
            }
            other => other,
        }
    }

    #[allow(dead_code)] // called only when the `fdsan` feature is on
    fn report_fdsan(&self, mismatch: Mismatch, action: Action) {
        if action == Action::None {
            return;
        }
        let mut line = Vec::new();
        line.extend_from_slice(b"fdsan: ");
        line.extend_from_slice(mismatch.message().as_bytes());
        line.extend_from_slice(b"\n");
        let _ = self.backend.debug_write(&line);
        if action == Action::Abort {
            self.backend.exit_abort(134);
        }
    }

    /// `closefrom`: close every descriptor `>= first`.
    pub fn close_from(&self, first: u32) -> Result<(), Errno> {
        let handles = {
            let _guard = guard(&self.lock);
            let removed = self.fd.borrow_mut().close_from(first);
            removed
                .iter()
                .flat_map(|(_, entry)| self.take_close_handles(entry.ofd))
                .collect::<Vec<_>>()
        };
        for handle in handles {
            self.close_handle(handle)?;
        }
        Ok(())
    }

    /// `close_range`.
    pub fn close_range(&self, first: u32, last: u32) -> Result<(), Errno> {
        let handles = {
            let _guard = guard(&self.lock);
            let removed = self.fd.borrow_mut().close_range(first, last);
            removed
                .iter()
                .flat_map(|(_, entry)| self.take_close_handles(entry.ofd))
                .collect::<Vec<_>>()
        };
        for handle in handles {
            self.close_handle(handle)?;
        }
        Ok(())
    }

    /// Set or clear `FD_CLOEXEC`.
    pub fn set_cloexec(&self, fd: i32, cloexec: bool) -> Result<(), Errno> {
        let _guard = guard(&self.lock);
        self.fd.borrow_mut().set_cloexec(fd, cloexec)
    }

    /// Whether `FD_CLOEXEC` is set.
    pub fn cloexec(&self, fd: i32) -> Result<bool, Errno> {
        let table = self.fd.borrow();
        let entry = table.entry(fd).ok_or(Errno::EBADF)?;
        if entry.is_free() {
            return Err(Errno::EBADF);
        }
        Ok(entry.cloexec())
    }

    /// Set the fdsan owner tag.
    pub fn set_tag(&self, fd: i32, tag: u64) -> Result<u64, Errno> {
        let _guard = guard(&self.lock);
        self.fd.borrow_mut().set_tag(fd, tag)
    }

    /// The fdsan owner tag.
    pub fn tag(&self, fd: i32) -> u64 {
        self.fd.borrow().tag(fd)
    }

    /// Number of open fds.
    pub fn open_count(&self) -> usize {
        self.fd.borrow().open_count()
    }

    /// Number of live OFDs.
    pub fn ofd_count(&self) -> usize {
        self.ofd.borrow().live()
    }

    /// Sum of all OFD reference counts.
    pub fn ofd_refs(&self) -> u32 {
        self.ofd.borrow().total_refs()
    }

    // ----------------------------------------------------------------- fcntl

    /// `fcntl(fd, cmd, arg)`.
    pub fn fcntl(&self, fd: i32, command: i32, arg: i64) -> Result<i64, Errno> {
        use xlat::FcntlAction;
        match xlat::fcntl_action(command) {
            FcntlAction::DupFd | FcntlAction::DupFdCloexec => {
                let min = u32::try_from(arg).map_err(|_| Errno::EINVAL)?;
                let limits = self.rlimits.get();
                if min >= limits.nofile_soft {
                    return Err(Errno::EINVAL);
                }
                let cloexec = matches!(xlat::fcntl_action(command), FcntlAction::DupFdCloexec);
                Ok(i64::from(self.dup(fd, min, cloexec)?))
            }
            FcntlAction::GetFd => {
                let table = self.fd.borrow();
                let entry = table.entry(fd).ok_or(Errno::EBADF)?;
                if entry.is_free() {
                    return Err(Errno::EBADF);
                }
                Ok(i64::from(entry.fd_flags))
            }
            FcntlAction::SetFd => {
                let cloexec = arg & i64::from(FD_CLOEXEC) != 0;
                self.set_cloexec(fd, cloexec)?;
                Ok(0)
            }
            FcntlAction::GetFl => {
                let reference = self.get(fd)?;
                Ok(i64::from(reference.status()))
            }
            FcntlAction::SetFl => {
                let reference = self.get(fd)?;
                let mutable = (fcntl::O_APPEND | fcntl::O_NONBLOCK | fcntl::O_NOATIME) as u32;
                let requested =
                    u32::try_from(arg & i64::from(mutable)).map_err(|_| Errno::EINVAL)?;
                let old = reference.status();
                let new = (old & !mutable) | requested;
                reference.set_status(new);
                Ok(0)
            }
            FcntlAction::GetLk
            | FcntlAction::SetLk
            | FcntlAction::SetLkw
            | FcntlAction::OfdGetLk
            | FcntlAction::OfdSetLk
            | FcntlAction::OfdSetLkw => {
                let reference = self.get(fd)?;
                let _ = reference;
                Err(Errno::ENOSYS)
            }
            FcntlAction::GetPipeSz => {
                let reference = self.get(fd)?;
                if reference.kind() == OfdKind::Pipe {
                    Ok(65536)
                } else {
                    Err(Errno::EINVAL)
                }
            }
            FcntlAction::SetPipeSz => {
                let reference = self.get(fd)?;
                if reference.kind() == OfdKind::Pipe {
                    Err(Errno::EPERM)
                } else {
                    Err(Errno::EINVAL)
                }
            }
            FcntlAction::AddSeals | FcntlAction::GetSeals => Err(Errno::EINVAL),
            FcntlAction::Invalid => Err(Errno::EINVAL),
        }
    }

    // ----------------------------------------------------------------- open

    /// `openat`: resolve, encode the wire path, call the backend, install an fd.
    pub fn openat(&self, dirfd: i32, path: &[u8], flags: u32, mode: u32) -> Result<i32, Errno> {
        if path.starts_with(b"/")
            && let Some(special) = special_fd(path)
        {
            return self.dup(target_fd(special), 0, false);
        }
        let base = self.base_for(dirfd)?;
        let resolved =
            resolve(&base, path, false, &self.limits).map_err(crate::path::path_errno)?;
        let spec = xlat::open_flags(flags)?;
        // `/dev/tty` names the process's controlling terminal, not a VFS node. FreeBSD `cttyopen`
        // resolves it from the session's controlling vnode; Raven asks the kernel for the live
        // terminal handle. With no controlling terminal the kernel answers `ENXIO` (POSIX).
        if resolved.abs == b"/dev/tty" {
            match self.backend.ctty_handle() {
                Ok(handle) => {
                    let path_box: Box<[u8]> = resolved.abs.clone().into_boxed_slice();
                    return match self.install(
                        handle,
                        OfdKind::Tty,
                        spec.status,
                        Some(path_box),
                        spec.cloexec,
                    ) {
                        Ok(fd) => Ok(fd),
                        Err(error) => {
                            let _ = self.backend.close(handle);
                            Err(error)
                        }
                    };
                }
                Err(Errno::ENOSYS) => {}
                Err(error) => return Err(error),
            }
        }
        let mut wire = spec.wire;
        if resolved.must_be_dir {
            if flags & fcntl::O_CREAT as u32 != 0 {
                return Err(Errno::EISDIR);
            }
            wire |= raven_proto_fs::WIRE_OPEN_DIRECTORY;
        }
        // `/dev/pts/<N>` is the public pty slave name and `/dev/ptmx` the clone device; the pty
        // server is the `pty:` scheme. Keep the public path on the OFD (ttyname) and send the
        // scheme path to the backend.
        let pty_wire = pty_wire(&resolved.abs);
        let wire_path = match &pty_wire {
            Some(wire) => wire.clone(),
            None => file_path(&resolved.abs),
        };
        let handle = crate::intr::cancellable(|| {
            crate::intr::run(
                crate::intr::CallClass::Restartable,
                crate::intr::restart_wanted(),
                || self.backend.open(&wire_path, u32::from(wire)),
            )
        })?;
        let kind = if pty_wire.is_some() || is_tty_path(&resolved.abs) {
            OfdKind::Tty
        } else if wire & raven_proto_fs::WIRE_OPEN_DIRECTORY != 0 {
            OfdKind::Dir
        } else {
            OfdKind::File
        };
        let path_box: Box<[u8]> = resolved.abs.clone().into_boxed_slice();
        let fd = match self.install(handle, kind, spec.status, Some(path_box), spec.cloexec) {
            Ok(fd) => fd,
            Err(error) => {
                let _ = self.backend.close(handle);
                return Err(error);
            }
        };
        // The public clone/master path carries the `pty:ptmx` wire name, so the OFD records the
        // terminal role even though it remembers the public path for `ttyname`.
        if pty_wire.as_deref().is_some_and(is_pty_master_wire)
            && let Ok(reference) = self.get(fd)
        {
            reference.set_pty_master();
        }
        if flags & fcntl::O_CREAT as u32 != 0 {
            // PG-24: protocol v1 has no mode on the wire; apply `mode & ~umask` after the create.
            let wanted = (mode & 0o777) & !self.umask.get();
            let _ = self.root_call(Call::Chmod {
                mode: wanted,
                path: &resolved.abs,
            });
        }
        Ok(fd)
    }

    /// `open` relative to the cwd.
    pub fn open(&self, path: &[u8], flags: u32, mode: u32) -> Result<i32, Errno> {
        self.openat(fcntl::AT_FDCWD, path, flags, mode)
    }

    /// `creat(path, mode)`.
    pub fn creat(&self, path: &[u8], mode: u32) -> Result<i32, Errno> {
        self.open(
            path,
            (fcntl::O_WRONLY | fcntl::O_CREAT | fcntl::O_TRUNC) as u32,
            mode,
        )
    }

    pub(crate) fn base_for(&self, dirfd: i32) -> Result<Vec<u8>, Errno> {
        if dirfd == fcntl::AT_FDCWD {
            return Ok(self.cwd.get());
        }
        let reference = self.get(dirfd)?;
        if !self.reference_is_directory(dirfd, &reference)? {
            return Err(Errno::ENOTDIR);
        }
        match reference.path() {
            Some(path) => Ok(path.to_vec()),
            // PG-31/32 add FPATH; an adopted directory without a remembered path cannot be used.
            None => Err(Errno::EOPNOTSUPP),
        }
    }

    // ----------------------------------------------------------------- read/write

    /// `read`, with the libc access-mode check (never leak the kernel's `EACCES`, A-sys-25).
    pub fn read(&self, fd: i32, buf: &mut [u8]) -> Result<usize, Errno> {
        let reference = self.get(fd)?;
        if !reference.snapshot_readable() {
            return Err(Errno::EBADF);
        }
        let handle = reference.handle();
        let blocking = reference.status() & fcntl::O_NONBLOCK as u32 == 0;
        // Any OFD with an attached scheme event is a waitable scheme socket. The kind is not a
        // sufficient test: `scheme::probe_kind` reports `Unknown` for a net socket adopted from
        // `SCM_RIGHTS`, even after its readiness event is attached (K438).
        let doorbell = reference.poll_signaled();
        let pipe = reference.kind() == OfdKind::Pipe;
        loop {
            let result = crate::intr::cancellable(|| {
                crate::intr::run(
                    crate::intr::CallClass::Restartable,
                    crate::intr::restart_wanted(),
                    || self.backend.read(handle, buf),
                )
            });
            // A terminal master read of zero past a real buffer is the pty hangup (EOF). Keep it
            // sticky so the next `poll` reports `POLLHUP` even though the scheme-file readiness
            // fallback still reports the master as readable (PG-25 is the server-side fix). The
            // OFD role is recorded at open time, so both `posix_openpt` and the `/dev/ptmx`
            // SchemeFile master qualify; the slave and the console must not inherit a master
            // hangup from a slave-side VEOF/^D read.
            if matches!(&result, Ok(0))
                && reference.kind() == OfdKind::Tty
                && reference.is_pty_master()
                && !buf.is_empty()
            {
                reference.set_eof();
            }
            match result {
                Err(Errno::EAGAIN) if blocking && (doorbell || pipe) => {
                    // The kernel pipe end is created non-blocking (there is no runtime setter);
                    // POSIX blocking is emulated from the OFD status here. The socket doorbell is
                    // also a hint: netd may have drained the queue before this read.
                    let ready = crate::poll::wait_readable(self, fd, -1)?;
                    if ready == 0 {
                        continue;
                    }
                }
                other => return other,
            }
        }
    }

    /// The fd currently attached to a socket handle, if any.
    ///
    /// The net adapter receives a kernel `Sock` (handle), while `poll` is fd-based. This lookup
    /// closes that gap for `accept`/`recv` waits without exposing the whole table.
    pub fn fd_for_socket_handle(&self, handle: Handle) -> Option<i32> {
        let entries = self.fd.borrow().open_entries();
        let arena = self.ofd.borrow();
        entries.into_iter().find_map(|(fd, entry)| {
            let ofd = arena.get(entry.ofd)?;
            (ofd.kind == OfdKind::Socket && ofd.handle.get() == handle).then_some(fd)
        })
    }

    /// `write`.
    pub fn write(&self, fd: i32, buf: &[u8]) -> Result<usize, Errno> {
        let reference = self.get(fd)?;
        if !reference.snapshot_writable() {
            return Err(Errno::EBADF);
        }
        let handle = reference.handle();
        let pipe = reference.kind() == OfdKind::Pipe;
        let blocking = reference.status() & fcntl::O_NONBLOCK as u32 == 0;
        if pipe && blocking && !buf.is_empty() {
            // The kernel pipe end is always non-blocking (see `pipe2`); a blocking descriptor
            // waits here for ring space and loops until every byte is accepted.
            self.write_pipe_blocking(fd, handle, buf)
        } else {
            crate::intr::cancellable(|| {
                crate::intr::run(
                    crate::intr::CallClass::Restartable,
                    crate::intr::restart_wanted(),
                    || self.backend.write(handle, buf),
                )
            })
        }
    }

    /// Emulate a blocking pipe write over the non-blocking kernel endpoint.
    fn write_pipe_blocking(&self, fd: i32, handle: Handle, buf: &[u8]) -> Result<usize, Errno> {
        let mut done = 0usize;
        while done < buf.len() {
            let remaining = &buf[done..];
            let result = crate::intr::cancellable(|| {
                crate::intr::run(
                    crate::intr::CallClass::Restartable,
                    crate::intr::restart_wanted(),
                    || self.backend.write(handle, remaining),
                )
            });
            match result {
                Ok(0) => break,
                Ok(count) => done = done.saturating_add(count.min(remaining.len())),
                Err(Errno::EAGAIN) => {
                    let ready = crate::poll::wait_writable(self, fd, -1)?;
                    if ready == 0 {
                        continue;
                    }
                }
                Err(Errno::EINTR) if done == 0 && crate::intr::restart_wanted() => continue,
                Err(error) => {
                    if done == 0 {
                        return Err(error);
                    }
                    break;
                }
            }
        }
        Ok(done)
    }

    /// `readv`: one request up to 64 KiB, then a loop for the rest.
    pub fn readv(&self, fd: i32, bufs: &mut [&mut [u8]]) -> Result<usize, Errno> {
        if bufs.is_empty() {
            return Ok(0);
        }
        let reference = self.get(fd)?;
        if !reference.snapshot_readable() {
            return Err(Errno::EBADF);
        }
        let handle = reference.handle();
        let total: usize = bufs.iter().map(|buf| buf.len()).sum();
        if total <= crate::io::BOUNCE_LIMIT {
            let mut bounce = alloc::vec![0u8; total];
            let count = crate::intr::cancellable(|| {
                crate::intr::run(
                    crate::intr::CallClass::Restartable,
                    crate::intr::restart_wanted(),
                    || self.backend.read(handle, &mut bounce),
                )
            })?;
            let mut offset = 0usize;
            for buf in bufs.iter_mut() {
                if offset >= count {
                    break;
                }
                let available = count.saturating_sub(offset);
                let copied = available.min(buf.len());
                if let (Some(source), Some(target)) = (
                    bounce.get(offset..offset.saturating_add(copied)),
                    buf.get_mut(..copied),
                ) {
                    target.copy_from_slice(source);
                }
                offset = offset.saturating_add(copied);
            }
            return Ok(count);
        }
        let mut total_read = 0usize;
        for buf in bufs.iter_mut() {
            if buf.is_empty() {
                continue;
            }
            let count = self.read(fd, buf)?;
            total_read = total_read.saturating_add(count);
            if count < buf.len() {
                break;
            }
        }
        Ok(total_read)
    }

    /// `writev`: one request up to 64 KiB, then a loop for the rest.
    pub fn writev(&self, fd: i32, bufs: &[&[u8]]) -> Result<usize, Errno> {
        if bufs.is_empty() {
            return Ok(0);
        }
        let reference = self.get(fd)?;
        if !reference.snapshot_writable() {
            return Err(Errno::EBADF);
        }
        let handle = reference.handle();
        let total: usize = bufs.iter().map(|buf| buf.len()).sum();
        if total <= crate::io::BOUNCE_LIMIT {
            let mut bounce = alloc::vec![0u8; total];
            let mut offset = 0usize;
            for buf in bufs {
                let end = offset.saturating_add(buf.len());
                if let Some(target) = bounce.get_mut(offset..end) {
                    target.copy_from_slice(buf);
                }
                offset = end;
            }
            return crate::intr::cancellable(|| {
                crate::intr::run(
                    crate::intr::CallClass::Restartable,
                    crate::intr::restart_wanted(),
                    || self.backend.write(handle, &bounce),
                )
            });
        }
        let mut total_written = 0usize;
        for buf in bufs {
            if buf.is_empty() {
                continue;
            }
            let count = self.write(fd, buf)?;
            total_written = total_written.saturating_add(count);
            if count < buf.len() {
                break;
            }
        }
        Ok(total_written)
    }

    /// `pread`: `ENOSYS` until PG-22; `ESPIPE` on pipes.
    pub fn pread(&self, fd: i32, _buf: &mut [u8], _offset: i64) -> Result<usize, Errno> {
        let reference = self.get(fd)?;
        if reference.kind() == OfdKind::Pipe {
            return Err(Errno::ESPIPE);
        }
        Err(Errno::ENOSYS)
    }

    /// `pwrite`: `ENOSYS` until PG-22; `ESPIPE` on pipes.
    pub fn pwrite(&self, fd: i32, _buf: &[u8], _offset: i64) -> Result<usize, Errno> {
        let reference = self.get(fd)?;
        if reference.kind() == OfdKind::Pipe {
            return Err(Errno::ESPIPE);
        }
        Err(Errno::ENOSYS)
    }

    /// `lseek`: move the server cursor and return the new offset.
    pub fn lseek(&self, fd: i32, offset: i64, whence: i32) -> Result<i64, Errno> {
        if !matches!(whence, 0..=2) {
            return Err(Errno::EINVAL);
        }
        let reference = self.get(fd)?;
        if matches!(reference.kind(), OfdKind::Pipe | OfdKind::Tty) {
            return Err(Errno::ESPIPE);
        }
        let handle = reference.handle();
        drop(reference);
        let whence = u32::try_from(whence).map_err(|_| Errno::EINVAL)?;
        let value = self.backend.seek(handle, offset, whence)?;
        i64::try_from(value).map_err(|_| Errno::EOVERFLOW)
    }

    // ----------------------------------------------------------------- metadata

    /// Raw `FSTAT` record followed by the libc conversion.
    pub fn fstat(&self, fd: i32) -> Result<Stat, Errno> {
        let reference = self.get(fd)?;
        let mut record = [0u8; raven_proto_fs::STAT_LEN];
        self.backend.fstat(reference.handle(), &mut record)?;
        let wire = WireStat::decode(&record).map_err(|error| error.errno())?;
        Ok(crate::stat::from_wire(
            &wire,
            StatSource::from_kind(reference.kind()),
        ))
    }

    /// `stat`: interim `open + FSTAT + close` (3.14 sequence 7); one `OP_STAT` after PG-31.
    pub fn stat(&self, path: &[u8]) -> Result<Stat, Errno> {
        self.stat_at(fcntl::AT_FDCWD, path, 0)
    }

    /// `lstat`: interim open with `O_NOFOLLOW`.
    pub fn lstat(&self, path: &[u8]) -> Result<Stat, Errno> {
        self.stat_at(fcntl::AT_FDCWD, path, fcntl::AT_SYMLINK_NOFOLLOW)
    }

    /// One-call `Lstat` on the cached `file:/` root. The server returns the fixed `Stat`
    /// record; `ENOSYS`/`EPROTO` keeps the old open+FSTAT+close fallback below.
    fn root_stat(&self, path: &[u8]) -> Result<Stat, Errno> {
        let mut request = [0u8; raven_proto_fs::CALL_HEADER_LEN + 2 + raven_proto_fs::MAX_PATH];
        let encoded = CallRequest::new(Call::Lstat { path })
            .encode(&mut request)
            .map_err(|error| error.errno())?;
        let (handle, reopened) = self.root_handle()?;
        let mut response = [0u8; 128];
        let count = match self
            .backend
            .call(handle, &request[..encoded], &mut response)
        {
            Err(Errno::EIO) if !reopened => {
                self.invalidate_root();
                let (handle, _) = self.root_handle()?;
                self.backend
                    .call(handle, &request[..encoded], &mut response)?
            }
            other => other?,
        };
        match CallResponse::decode(raven_proto_fs::OP_LSTAT, &response[..count])
            .map_err(|error| error.errno())?
        {
            CallResponse::Stat { status: 0, stat } => {
                Ok(crate::stat::from_wire(&stat, StatSource::Unknown))
            }
            CallResponse::Stat { status, .. } => match xlat::call_status(status) {
                Err(error) => Err(error),
                Ok(()) => Err(Errno::EPROTO),
            },
            _ => Err(Errno::EPROTO),
        }
    }

    /// `fstatat`.
    pub fn stat_at(&self, dirfd: i32, path: &[u8], flags: i32) -> Result<Stat, Errno> {
        if path.is_empty() && flags & fcntl::AT_EMPTY_PATH != 0 {
            return self.fstat(dirfd);
        }
        if flags & fcntl::AT_SYMLINK_NOFOLLOW != 0 {
            let absolute = self.absolute_at(dirfd, path)?;
            if let Ok(stat) = self.root_stat(&absolute) {
                return Ok(stat);
            }
        }
        let mut open_flags = fcntl::O_RDONLY as u32;
        if flags & fcntl::AT_SYMLINK_NOFOLLOW != 0 {
            open_flags |= fcntl::O_NOFOLLOW as u32;
        }
        let fd = self.openat(dirfd, path, open_flags, 0)?;
        let result = self.fstat(fd);
        let _ = self.close(fd);
        result
    }

    /// `access`/`faccessat`: mode bits against the cached credentials (uid 0 until PG-28).
    pub fn access(&self, path: &[u8], mode: i32) -> Result<(), Errno> {
        if mode & !(fcntl::R_OK | fcntl::W_OK | fcntl::X_OK) != 0 {
            return Err(Errno::EINVAL);
        }
        let stat = self.stat(path)?;
        if mode & fcntl::X_OK != 0 {
            let execute = raven_libc_abi::stat::S_IXUSR
                | raven_libc_abi::stat::S_IXGRP
                | raven_libc_abi::stat::S_IXOTH;
            if stat.st_mode & execute == 0 {
                return Err(Errno::EACCES);
            }
        }
        Ok(())
    }

    /// `chmod` through `OP_CHMOD` on the root handle.
    pub fn chmod(&self, path: &[u8], mode: u32) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(path, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        // The public pty slave name is a scheme endpoint, not a VFS node: the pty server owns
        // the node metadata (`fstat` reports a root-owned 0600 chr device). FreeBSD devfs allows
        // `chmod` on the pts node; Raven models that node as read-only, and OpenSSH
        // `sshpty.c` `pty_setowner` explicitly tolerates `EROFS` for a root-owned tty whose mode
        // has no group/other bits.
        if pty_slave_wire(&resolved.abs).is_some() {
            return Err(Errno::EROFS);
        }
        self.root_call(Call::Chmod {
            mode: mode & 0o7777,
            path: &resolved.abs,
        })
    }

    /// Resolve a path relative to `dirfd` (or the cwd) for the `*at` calls.
    fn absolute_at(&self, dirfd: i32, path: &[u8]) -> Result<Vec<u8>, Errno> {
        if dirfd == fcntl::AT_FDCWD {
            return self
                .cwd
                .resolve(path, false, &self.limits)
                .map(|resolved| resolved.abs)
                .map_err(crate::path::path_errno);
        }
        let mut base = self.base_for(dirfd)?;
        if base.last() != Some(&b'/') {
            base.push(b'/');
        }
        base.extend_from_slice(path);
        Ok(base)
    }

    /// `chown` through `OP_CHOWN` on the root handle.
    pub fn chown(&self, path: &[u8], uid: u32, gid: u32) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(path, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        // See `chmod`: the pty slave node metadata is server-owned and read-only. OpenSSH's
        // `pty_setowner` treats `EROFS` on a root-owned tty as "warn and continue".
        if pty_slave_wire(&resolved.abs).is_some() {
            return Err(Errno::EROFS);
        }
        self.root_call(Call::Chown {
            uid,
            gid,
            path: &resolved.abs,
        })
    }

    /// `lchown` through `OP_LCHOWN` on the root handle.
    pub fn lchown(&self, path: &[u8], uid: u32, gid: u32) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(path, true, &self.limits)
            .map_err(crate::path::path_errno)?;
        self.root_call(Call::Lchown {
            uid,
            gid,
            path: &resolved.abs,
        })
    }

    /// `fchown`: use the OFD path and `OP_CHOWN`, like `fchmod`.
    pub fn fchown(&self, fd: i32, uid: u32, gid: u32) -> Result<(), Errno> {
        let reference = self.get(fd)?;
        let path = reference.path().ok_or(Errno::EOPNOTSUPP)?;
        drop(reference);
        self.root_call(Call::Chown {
            uid,
            gid,
            path: &path,
        })
    }

    /// `mkfifoat` through `OP_MKFIFO`.
    pub fn mkfifoat(&self, dirfd: i32, path: &[u8], mode: u32) -> Result<(), Errno> {
        let resolved = self.absolute_at(dirfd, path)?;
        self.root_call(Call::Mkfifo {
            mode: mode & 0o7777,
            path: &resolved,
        })
    }

    /// `mknodat` through `OP_MKNOD`.
    pub fn mknodat(&self, dirfd: i32, path: &[u8], mode: u32, dev: u32) -> Result<(), Errno> {
        let resolved = self.absolute_at(dirfd, path)?;
        self.root_call(Call::Mknod {
            mode,
            dev,
            path: &resolved,
        })
    }

    /// `utime` through `OP_UTIMENS`; `None` omits that timestamp.
    pub fn utime(&self, path: &[u8], atime: Option<i64>, mtime: Option<i64>) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(path, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        self.root_call(Call::Utimens {
            path: &resolved.abs,
            atime: atime.map_or(SetTime::Omit, SetTime::Set),
            mtime: mtime.map_or(SetTime::Omit, SetTime::Set),
            follow: true,
        })
    }

    /// `getdtablesize`: the soft `RLIMIT_NOFILE` value.
    pub fn getdtablesize(&self) -> i32 {
        i32::try_from(self.rlimits().nofile_soft).unwrap_or(i32::MAX)
    }

    /// `gettid`: the calling thread's stable tid (`THREAD_SELF`).
    pub fn gettid(&self) -> Result<i32, Errno> {
        raven_libc_sys::sys::thread_self().map(|tid| i32::try_from(tid).unwrap_or(i32::MAX))
    }

    /// `gethostid`: `sys:kern/hostid`, or zero when the sysctl is absent.
    pub fn gethostid(&self) -> i32 {
        let Ok(bytes) = self.read_sys(b"kern/hostid") else {
            return 0;
        };
        let Ok(text) = core::str::from_utf8(&bytes) else {
            return 0;
        };
        text.trim().parse::<i32>().unwrap_or(0)
    }

    /// `getdomainname`: `sys:kern/domainname`, truncated to the caller buffer.
    pub fn getdomainname(&self, buf: &mut [u8]) -> Result<(), Errno> {
        let value = self.read_sys(b"kern/domainname").unwrap_or_default();
        let line = value.split(|byte| *byte == b'\n').next().unwrap_or(&[]);
        let end = line
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
            .map_or(0, |index| index.saturating_add(1));
        let value = line.get(..end).unwrap_or(&[]);
        let length = value.len().min(buf.len().saturating_sub(1));
        buf.get_mut(..length)
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(value.get(..length).unwrap_or(&[]));
        if let Some(slot) = buf.get_mut(length) {
            *slot = 0;
        }
        Ok(())
    }

    /// `setdomainname`: write `sys:kern/domainname`.
    pub fn setdomainname(&self, name: &[u8]) -> Result<(), Errno> {
        if name.len() > 64 || name.contains(&0) {
            return Err(Errno::EINVAL);
        }
        self.write_sys(b"kern/domainname", name)
    }

    /// `fchmod`: change the permission bits of the open file.    ///
    /// `OP_CHMOD` is directory-relative: the scheme resolves its path against
    /// the handle it is sent on, and an attached file handle is not a
    /// directory, so the handle-relative `"."` form is rejected (`EINVAL` in
    /// the ramfs name check). The protocol has no handle-based `FCHMOD`, so
    /// send `OP_CHMOD` through the VFS root (`file:/`) with the path the OFD
    /// was opened with. The mode is masked to the permission bits, matching
    /// `chmod`.
    ///
    /// A descriptor without a remembered path (an adopted descriptor, PG-31)
    /// has no path to name and returns `EOPNOTSUPP` until the protocol grows a
    /// handle-based operation.
    pub fn fchmod(&self, fd: i32, mode: u32) -> Result<(), Errno> {
        let reference = self.get(fd)?;
        let path = reference.path().ok_or(Errno::EOPNOTSUPP)?;
        drop(reference);
        self.root_call(Call::Chmod {
            mode: mode & 0o7777,
            path: &path,
        })
    }

    /// `truncate(path, size)`: open for write, `OP_TRUNCATE`, close.
    pub fn truncate(&self, path: &[u8], size: i64) -> Result<(), Errno> {
        if size < 0 {
            return Err(Errno::EINVAL);
        }
        let fd = self.open(path, fcntl::O_WRONLY as u32, 0)?;
        let result = self.ftruncate(fd, size);
        let _ = self.close(fd);
        result
    }

    /// `ftruncate` through `OP_TRUNCATE` on the handle.
    pub fn ftruncate(&self, fd: i32, size: i64) -> Result<(), Errno> {
        if size < 0 {
            return Err(Errno::EINVAL);
        }
        let reference = self.get(fd)?;
        if !reference.snapshot_writable() {
            return Err(Errno::EINVAL);
        }
        let mut request = [0u8; 64];
        let encoded = CallRequest::new(Call::Truncate { size: size as u64 })
            .encode(&mut request)
            .map_err(|error| error.errno())?;
        let mut response = [0u8; 8];
        let count = self
            .backend
            .call(reference.handle(), &request[..encoded], &mut response)?;
        let call_response = CallResponse::decode(raven_proto_fs::OP_TRUNCATE, &response[..count])
            .map_err(|error| error.errno())?;
        xlat::call_status(call_response.status())
    }

    /// `fsync`/`fdatasync` through the global `OP_SYNC` until PG-21 adds `OP_FSYNC`.
    pub fn fsync(&self, fd: i32) -> Result<(), Errno> {
        let reference = self.get(fd)?;
        let _ = reference;
        self.root_call(Call::Sync)
    }

    /// `sync`.
    pub fn sync(&self) -> Result<(), Errno> {
        self.root_call(Call::Sync)
    }

    /// `posix_fadvise`: advice only, `0`; `EBADF` for a bad fd, `ESPIPE` for pipes.
    pub fn posix_fadvise(
        &self,
        fd: i32,
        _offset: i64,
        _len: i64,
        _advice: i32,
    ) -> Result<(), Errno> {
        let reference = self.get(fd)?;
        if reference.kind() == OfdKind::Pipe {
            return Err(Errno::ESPIPE);
        }
        Ok(())
    }

    /// `posix_fallocate`: `EOPNOTSUPP` until PG-21; never emulate by writing zeros.
    pub fn posix_fallocate(&self, fd: i32, _offset: i64, _len: i64) -> Result<(), Errno> {
        let reference = self.get(fd)?;
        let _ = reference;
        Err(Errno::EOPNOTSUPP)
    }

    // ----------------------------------------------------------------- directories

    /// `mkdir`.
    pub fn mkdir(&self, path: &[u8], mode: u32) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(path, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        let mode = (mode & 0o7777) & !self.umask.get();
        self.root_call(Call::Mkdir {
            mode,
            path: &resolved.abs,
        })
    }

    /// `rmdir`.
    pub fn rmdir(&self, path: &[u8]) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(path, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        self.root_call(Call::Rmdir {
            path: &resolved.abs,
        })
    }

    /// `unlink`: a trailing slash is `EISDIR` (Linux) before the server is asked.
    pub fn unlink(&self, path: &[u8]) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(path, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        if resolved.must_be_dir {
            return Err(Errno::EISDIR);
        }
        self.root_call(Call::Unlink {
            path: &resolved.abs,
        })
    }

    /// `unlinkat`; `AT_REMOVEDIR` selects `OP_RMDIR`.
    pub fn unlinkat(&self, dirfd: i32, path: &[u8], flags: i32) -> Result<(), Errno> {
        if flags & !fcntl::AT_REMOVEDIR != 0 {
            return Err(Errno::EINVAL);
        }
        let base = self.base_for(dirfd)?;
        let resolved =
            resolve(&base, path, false, &self.limits).map_err(crate::path::path_errno)?;
        if flags & fcntl::AT_REMOVEDIR != 0 {
            self.root_call(Call::Rmdir {
                path: &resolved.abs,
            })
        } else {
            if resolved.must_be_dir {
                return Err(Errno::EISDIR);
            }
            self.root_call(Call::Unlink {
                path: &resolved.abs,
            })
        }
    }

    /// `rename`: both paths resolve against the cwd; a trailing slash requires a directory.
    pub fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        let source = self
            .cwd
            .resolve(from, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        let target = self
            .cwd
            .resolve(to, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        if source.must_be_dir {
            let stat = self.stat(&source.abs)?;
            if stat.st_mode & raven_libc_abi::stat::S_IFMT != raven_libc_abi::stat::S_IFDIR {
                return Err(Errno::ENOTDIR);
            }
        }
        self.root_call(Call::Rename {
            from: &source.abs,
            to: &target.abs,
        })
    }

    /// `symlink`.
    pub fn symlink(&self, target: &[u8], link: &[u8]) -> Result<(), Errno> {
        if target.is_empty() {
            return Err(Errno::ENOENT);
        }
        let resolved = self
            .cwd
            .resolve(link, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        self.root_call(Call::Symlink {
            path: &resolved.abs,
            target,
        })
    }

    /// `readlink`.
    pub fn readlink(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        let resolved = self
            .cwd
            .resolve(path, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        let mut request = [0u8; raven_proto_fs::CALL_HEADER_LEN + 2 + raven_proto_fs::MAX_PATH];
        let encoded = CallRequest::new(Call::Readlink {
            path: &resolved.abs,
        })
        .encode(&mut request)
        .map_err(|error| error.errno())?;
        let (handle, reopened) = self.root_handle()?;
        let mut response = [0u8; 4 + 2 + raven_proto_fs::MAX_PATH];
        let count = match self
            .backend
            .call(handle, &request[..encoded], &mut response)
        {
            Err(Errno::EIO) if !reopened => {
                self.invalidate_root();
                let (handle, _) = self.root_handle()?;
                self.backend
                    .call(handle, &request[..encoded], &mut response)?
            }
            other => other?,
        };
        let call_response = CallResponse::decode(raven_proto_fs::OP_READLINK, &response[..count])
            .map_err(|error| error.errno())?;
        xlat::call_status(call_response.status())?;
        match call_response {
            CallResponse::Readlink { target, .. } => {
                let copied = target.len().min(buf.len());
                buf.get_mut(..copied)
                    .ok_or(Errno::EINVAL)?
                    .copy_from_slice(target.get(..copied).unwrap_or(&[]));
                Ok(copied)
            }
            _ => Err(Errno::EPROTO),
        }
    }

    // ----------------------------------------------------------------- cwd

    /// `chdir`: resolve, verify with an `O_DIRECTORY` open, then store.
    pub fn chdir(&self, path: &[u8]) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(path, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        let fd = self.open(
            &resolved.abs,
            (fcntl::O_RDONLY | fcntl::O_DIRECTORY) as u32,
            0,
        )?;
        let is_dir = self
            .get(fd)
            .map(|reference| reference.kind() == OfdKind::Dir)
            .unwrap_or(false);
        let _ = self.close(fd);
        if !is_dir {
            return Err(Errno::ENOTDIR);
        }
        self.cwd.set(&resolved.abs);
        Ok(())
    }

    /// `fchdir`: use the OFD's remembered path.
    pub fn fchdir(&self, fd: i32) -> Result<(), Errno> {
        let reference = self.get(fd)?;
        if !self.reference_is_directory(fd, &reference)? {
            return Err(Errno::ENOTDIR);
        }
        let path = reference.path().ok_or(Errno::EOPNOTSUPP)?;
        self.cwd.set(&path);
        Ok(())
    }

    fn reference_is_directory(&self, fd: i32, reference: &OfdRef<'_, B, L>) -> Result<bool, Errno> {
        if reference.kind() == OfdKind::Dir {
            return Ok(true);
        }
        if reference.kind() == OfdKind::File || reference.kind() == OfdKind::Unknown {
            let stat = self.fstat(fd)?;
            return Ok(stat.st_mode & raven_libc_abi::stat::S_IFMT == raven_libc_abi::stat::S_IFDIR);
        }
        Ok(false)
    }

    /// `getcwd` into a caller buffer; `ERANGE` when it is too small.
    pub fn getcwd_into(&self, buf: &mut [u8]) -> Result<usize, Errno> {
        let cwd = self.cwd.get();
        let needed = cwd.len().saturating_add(1);
        if buf.len() < needed {
            return Err(Errno::ERANGE);
        }
        buf.get_mut(..cwd.len())
            .ok_or(Errno::ERANGE)?
            .copy_from_slice(&cwd);
        if let Some(slot) = buf.get_mut(cwd.len()) {
            *slot = 0;
        }
        Ok(needed)
    }

    // ----------------------------------------------------------------- mount/statfs

    /// `mount(source, target, fstype, flags, data)`: `source` becomes `fstype + ":"`.
    pub fn mount(&self, source: &[u8], target: &[u8], fstype: &[u8]) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(target, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        let mut scheme = Vec::with_capacity(fstype.len() + 1);
        scheme.extend_from_slice(fstype);
        scheme.push(b':');
        if source.is_empty() {
            scheme.clear();
            scheme.extend_from_slice(fstype);
            scheme.push(b':');
        }
        self.root_call(Call::Mount {
            path: &resolved.abs,
            scheme: &scheme,
        })
    }

    /// `umount`.
    pub fn umount(&self, target: &[u8]) -> Result<(), Errno> {
        let resolved = self
            .cwd
            .resolve(target, false, &self.limits)
            .map_err(crate::path::path_errno)?;
        self.root_call(Call::Umount {
            path: &resolved.abs,
        })
    }

    fn statfs_on(&self, handle: Handle) -> Result<raven_proto_fs::StatFs, Errno> {
        let mut request = [0u8; raven_proto_fs::CALL_HEADER_LEN];
        let encoded = CallRequest::new(Call::Statfs)
            .encode(&mut request)
            .map_err(|error| error.errno())?;
        let mut response = [0u8; 4 + raven_proto_fs::STATFS_LEN];
        let count = self
            .backend
            .call(handle, &request[..encoded], &mut response)?;
        let call_response = CallResponse::decode(raven_proto_fs::OP_STATFS, &response[..count])
            .map_err(|error| error.errno())?;
        xlat::call_status(call_response.status())?;
        match call_response {
            CallResponse::StatFs { statfs, .. } => Ok(statfs),
            _ => Err(Errno::EPROTO),
        }
    }

    /// `fstatvfs`.
    pub fn fstatvfs(&self, fd: i32) -> Result<raven_libc_abi::structs::Statvfs, Errno> {
        let reference = self.get(fd)?;
        let wire = self.statfs_on(reference.handle())?;
        Ok(crate::stat::statvfs_from_wire(&wire))
    }

    /// `fstatfs`: the raw 36-byte record.
    pub fn fstatfs(&self, fd: i32) -> Result<raven_proto_fs::StatFs, Errno> {
        let reference = self.get(fd)?;
        self.statfs_on(reference.handle())
    }

    /// `statfs(path)`: open the path first so the answer belongs to that filesystem.
    pub fn statfs(&self, path: &[u8]) -> Result<raven_proto_fs::StatFs, Errno> {
        let fd = self.open(path, fcntl::O_RDONLY as u32, 0)?;
        let result = self.fstatfs(fd);
        let _ = self.close(fd);
        result
    }

    /// `statvfs(path)`: open the path first so the answer belongs to that filesystem (A-sys-18).
    pub fn statvfs(&self, path: &[u8]) -> Result<raven_libc_abi::structs::Statvfs, Errno> {
        let fd = self.open(path, fcntl::O_RDONLY as u32, 0)?;
        let result = self.fstatvfs(fd);
        let _ = self.close(fd);
        result
    }

    // ----------------------------------------------------------------- root calls

    fn root_handle(&self) -> Result<(Handle, bool), Errno> {
        if let Some(handle) = *self.root.borrow() {
            return Ok((handle, false));
        }
        let handle = self.backend.open(
            b"file:/",
            u32::from(raven_proto_fs::WIRE_OPEN_READ | raven_proto_fs::WIRE_OPEN_DIRECTORY),
        )?;
        *self.root.borrow_mut() = Some(handle);
        Ok((handle, true))
    }

    /// Drop the cached `file:/` root; a `chroot` must never reuse the old root.
    pub fn invalidate_root(&self) {
        if let Some(handle) = self.root.borrow_mut().take() {
            let _ = self.backend.close(handle);
        }
    }

    /// Send one path-control request on the cached `file:/` root handle; reopen once on `EIO`.
    pub fn root_call(&self, call: Call<'_>) -> Result<(), Errno> {
        let mut request =
            [0u8; raven_proto_fs::CALL_HEADER_LEN + 2 * (2 + raven_proto_fs::MAX_PATH)];
        let encoded = CallRequest::new(call)
            .encode(&mut request)
            .map_err(|error| error.errno())?;
        let (handle, reopened) = self.root_handle()?;
        let mut response = [0u8; 8];
        let count = match self
            .backend
            .call(handle, &request[..encoded], &mut response)
        {
            Err(Errno::EIO) if !reopened => {
                self.invalidate_root();
                let (handle, _) = self.root_handle()?;
                self.backend
                    .call(handle, &request[..encoded], &mut response)?
            }
            other => other?,
        };
        let op = match &call {
            Call::Mkdir { .. } => raven_proto_fs::OP_MKDIR,
            Call::Unlink { .. } => raven_proto_fs::OP_UNLINK,
            Call::Rmdir { .. } => raven_proto_fs::OP_RMDIR,
            Call::Rename { .. } => raven_proto_fs::OP_RENAME,
            Call::Truncate { .. } => raven_proto_fs::OP_TRUNCATE,
            Call::Symlink { .. } => raven_proto_fs::OP_SYMLINK,
            Call::Readlink { .. } => raven_proto_fs::OP_READLINK,
            Call::Chmod { .. } => raven_proto_fs::OP_CHMOD,
            Call::Sync => raven_proto_fs::OP_SYNC,
            Call::Mount { .. } => raven_proto_fs::OP_MOUNT,
            Call::Umount { .. } => raven_proto_fs::OP_UMOUNT,
            Call::Statfs => raven_proto_fs::OP_STATFS,
            Call::Access { .. } => raven_proto_fs::OP_ACCESS,
            Call::Link { .. } => raven_proto_fs::OP_LINK,
            Call::Chown { .. } => raven_proto_fs::OP_CHOWN,
            Call::Lchown { .. } => raven_proto_fs::OP_LCHOWN,
            Call::Fchown { .. } => raven_proto_fs::OP_FCHOWN,
            Call::Mkfifo { .. } => raven_proto_fs::OP_MKFIFO,
            Call::Mknod { .. } => raven_proto_fs::OP_MKNOD,
            Call::Utimens { .. } => raven_proto_fs::OP_UTIMENS,
            Call::Statvfs => raven_proto_fs::OP_STATVFS,
            Call::Lstat { .. } => raven_proto_fs::OP_LSTAT,
            Call::Mmap { .. } => raven_proto_fs::OP_MMAP,
        };
        let call_response =
            CallResponse::decode(op, &response[..count]).map_err(|error| error.errno())?;
        xlat::call_status(call_response.status())
    }

    // ----------------------------------------------------------------- inheritance

    /// Build the `FdMap` block for `fork`/`exec`.
    ///
    /// The caller transfers each distinct `(handle, rights)` first and passes the mapping from
    /// parent handle to child handle; this method records the child handle and the path string.
    pub fn build_fdmap(&self, mapping: &[(Handle, Handle)]) -> Result<FdMap, Errno> {
        let entries = self.fd.borrow().open_entries();
        let arena = self.ofd.borrow();
        let mut out = Vec::new();
        for (fd, entry) in entries {
            let ofd = arena.get(entry.ofd).ok_or(Errno::EBADF)?;
            if !transferable(ofd.kind) {
                continue;
            }
            let child = mapping
                .iter()
                .find(|(parent, _)| *parent == ofd.handle.get())
                .map(|(_, child)| *child)
                .ok_or(Errno::EBADF)?;
            let mut inherited = FdMapEntry::new(
                fd,
                child.0,
                ofd.kind,
                entry.fd_flags,
                ofd.status.load(core::sync::atomic::Ordering::Relaxed),
            );
            inherited.path = ofd.path.as_ref().map(|path| path.to_vec());
            inherited.event = event_number(ofd.event.get());
            inherited.readiness = ofd.rearm.load(core::sync::atomic::Ordering::Relaxed);
            inherited.socket = ofd.socket_meta();
            inherited.pty_master = ofd.is_pty_master();
            out.push(inherited);
        }
        Ok(FdMap { entries: out })
    }

    /// Build the `NAME=hex` environment entry that carries the fd table across `execve`.
    ///
    /// `PROC_EXEC` keeps the kernel handles in the same process but replaces the image, so the
    /// new libc starts with an empty table. The environment entry carries the identity mapping
    /// (`fd -> handle`) that the new image restores before its first fd use. Non-transferable
    /// kinds and pending socket OFDs without a handle are skipped.
    pub fn exec_fdmap_env(&self) -> Result<Option<Vec<u8>>, Errno> {
        let entries = self.fd.borrow().open_entries();
        let arena = self.ofd.borrow();
        let mut block = FdMap::empty();
        for (fd, entry) in entries {
            let ofd = arena.get(entry.ofd).ok_or(Errno::EBADF)?;
            if !transferable(ofd.kind) || ofd.handle.get() == Handle::NONE {
                continue;
            }
            let mut inherited = FdMapEntry::new(
                fd,
                ofd.handle.get().0,
                ofd.kind,
                entry.fd_flags,
                ofd.status.load(core::sync::atomic::Ordering::Relaxed),
            );
            inherited.path = ofd.path.as_ref().map(|path| path.to_vec());
            // The parent may close its accepted fd and event immediately after fork, before the
            // child restores the FdMap. Give the child its own handle to the same UserEvent so
            // the parent's final close cannot invalidate it.
            let event = ofd.event.get();
            inherited.event = if event == Handle::NONE {
                0
            } else {
                self.backend
                    .dup(event, raven_abi::types::Rights::ALL.bits())
                    .map_or(event.0, |duplicate| duplicate.0)
            };
            inherited.readiness = ofd.rearm.load(core::sync::atomic::Ordering::Relaxed);
            inherited.socket = ofd.socket_meta();
            inherited.pty_master = ofd.is_pty_master();
            block.entries.push(inherited);
        }
        if block.entries.is_empty() {
            return Ok(None);
        }
        let value = block.encode_env()?;
        let mut out =
            Vec::with_capacity(ENV_NAME.len().saturating_add(1).saturating_add(value.len()));
        out.extend_from_slice(ENV_NAME);
        out.push(b'=');
        out.extend_from_slice(&value);
        Ok(Some(out))
    }

    /// `RSH_FD0/1/2` environment entries for the shell's standard descriptors.
    ///
    /// The first-party `rsh` takes its standard streams as scheme handles in these variables. A
    /// host that execs it directly (OpenSSH's command child) has only fd numbers, so publish the
    /// current fd 0/1/2 handles. The caller's existing `RSH_FD*` values win.
    pub fn stdio_env(&self) -> Vec<Vec<u8>> {
        let entries = self.fd.borrow().open_entries();
        let arena = self.ofd.borrow();
        let mut out = Vec::new();
        for (fd, entry) in entries {
            let name = match fd {
                0 => b"RSH_FD0".as_slice(),
                1 => b"RSH_FD1".as_slice(),
                2 => b"RSH_FD2".as_slice(),
                _ => continue,
            };
            let Some(ofd) = arena.get(entry.ofd) else {
                continue;
            };
            if !transferable(ofd.kind) {
                continue;
            }
            let handle = ofd.handle.get();
            if handle == Handle::NONE {
                continue;
            }
            let mut row = Vec::new();
            row.extend_from_slice(name);
            row.push(b'=');
            row.extend_from_slice(alloc::format!("{}", handle.0).as_bytes());
            out.push(row);
        }
        out
    }

    /// Close every descriptor marked `FD_CLOEXEC` after `execve` restored the table.
    ///
    /// The kernel keeps handles across exec; only the new libc knows which descriptors carried
    /// the flag. This runs once during startup, after [`Self::restore_fdmap`].
    pub fn close_cloexec_fds(&self) -> Result<(), Errno> {
        let open = self.fd.borrow().open_entries();
        for (fd, entry) in open {
            if entry.cloexec() {
                let _ = self.close(fd);
            }
        }
        Ok(())
    }

    /// Rebuild the table from an inheritance block whose handles are already valid in this
    /// process. The block is installed before `main`; the table starts empty (O-5).
    pub fn restore_fdmap(&self, block: &FdMap) -> Result<(), Errno> {
        let mut by_handle: alloc::collections::BTreeMap<u32, u32> =
            alloc::collections::BTreeMap::new();
        for entry in &block.entries {
            if !transferable(entry.kind) {
                return Err(Errno::EINVAL);
            }
            // A handle has exactly one OFD: fds that were dup'd from each other must share the
            // status flags and the close-at-last-reference lifetime.
            let ofd = if let Some(index) = by_handle.get(&entry.handle).copied() {
                self.ofd.borrow_mut().add_ref(index)?;
                index
            } else {
                let index = self.ofd.borrow_mut().insert(
                    Handle(entry.handle),
                    entry.kind,
                    entry.status,
                    entry.path.clone().map(Vec::into_boxed_slice),
                );
                if let Some(inherited) = self.ofd.borrow_mut().get_mut(index) {
                    if entry.event != 0 {
                        let _ = inherited.set_event(Handle(entry.event));
                    }
                    inherited.set_poll_rearm(entry.readiness);
                    if let Some(socket) = entry.socket {
                        inherited.set_socket_meta(socket);
                    }
                    if entry.pty_master {
                        inherited.set_pty_master();
                    }
                }
                by_handle.insert(entry.handle, index);
                index
            };
            if let Err(error) = self.fd.borrow_mut().install_at(
                entry.fd as u32,
                ofd,
                entry.fd_flags & FD_CLOEXEC != 0,
                0,
            ) {
                self.release_ofd_ref(ofd);
                return Err(error);
            }
        }
        Ok(())
    }

    /// `pipe`: create an in-kernel pipe with no flags.
    pub fn pipe(&self) -> Result<(i32, i32), Errno> {
        self.pipe2(0)
    }

    /// `pipe2`: create an in-kernel pipe and install its two handles in the fd table.
    ///
    /// The kernel accepts only `O_CLOEXEC` and `O_NONBLOCK`; every other bit is `EINVAL`, not a
    /// silent ignore. `O_NONBLOCK` is recorded in both OFD status words and `O_CLOEXEC` in both
    /// fd-table entries.
    ///
    /// The kernel pipe end captures its blocking flag at creation and has no runtime setter, while
    /// POSIX `fcntl(F_SETFL)` must change the open-file description. Create every kernel end
    /// non-blocking and emulate blocking in `read`/`write` from the OFD status; a later
    /// `F_SETFL(O_NONBLOCK)` then takes effect without a second kernel object.
    pub fn pipe2(&self, flags: i32) -> Result<(i32, i32), Errno> {
        let flags = u32::try_from(flags).map_err(|_| Errno::EINVAL)?;
        let known = (fcntl::O_CLOEXEC | fcntl::O_NONBLOCK) as u32;
        if flags & !known != 0 {
            return Err(Errno::EINVAL);
        }
        let kernel_flags = flags | fcntl::O_NONBLOCK as u32;
        let (read_handle, write_handle) = raven_libc_sys::sys::pipe2(kernel_flags)?;
        let cloexec = flags & fcntl::O_CLOEXEC as u32 != 0;
        let status = if flags & fcntl::O_NONBLOCK as u32 != 0 {
            fcntl::O_NONBLOCK as u32
        } else {
            0
        };
        let read = self.install(
            read_handle,
            OfdKind::Pipe,
            fcntl::O_RDONLY as u32 | status,
            None,
            cloexec,
        )?;
        let write = match self.install(
            write_handle,
            OfdKind::Pipe,
            fcntl::O_WRONLY as u32 | status,
            None,
            cloexec,
        ) {
            Ok(fd) => fd,
            Err(error) => {
                let _ = self.close(read);
                return Err(error);
            }
        };
        Ok((read, write))
    }

    /// `tcgetattr`.
    pub fn tcgetattr(&self, fd: i32) -> Result<raven_libc_abi::structs::Termios, Errno> {
        crate::ioctl::tty::tcgetattr(self, fd)
    }

    /// `tcsetattr`.
    pub fn tcsetattr(
        &self,
        fd: i32,
        termios: &raven_libc_abi::structs::Termios,
    ) -> Result<(), Errno> {
        crate::ioctl::tty::tcsetattr(self, fd, termios)
    }

    // ----------------------------------------------------------------- system information

    /// The cached `rand:` handle, opened on first use.
    pub(crate) fn random_handle(&self) -> Result<Handle, Errno> {
        if let Some(handle) = *self.rand.borrow() {
            return Ok(handle);
        }
        let handle = self
            .backend
            .open(b"rand:", u32::from(raven_proto_fs::WIRE_OPEN_READ))?;
        *self.rand.borrow_mut() = Some(handle);
        Ok(handle)
    }

    /// Read one `sys:` file, at most 4096 bytes.
    pub(crate) fn read_sys(&self, path: &[u8]) -> Result<Vec<u8>, Errno> {
        let mut full = Vec::with_capacity(path.len() + 4);
        full.extend_from_slice(b"sys:");
        full.extend_from_slice(path);
        let handle = self
            .backend
            .open(&full, u32::from(raven_proto_fs::WIRE_OPEN_READ))?;
        let mut out = vec![0u8; 4096];
        let count = match self.backend.read(handle, &mut out) {
            Ok(count) => count,
            Err(error) => {
                let _ = self.backend.close(handle);
                return Err(error);
            }
        };
        let _ = self.backend.close(handle);
        out.truncate(count);
        Ok(out)
    }

    /// Write one `sys:` file, appending a newline for the decimal text protocol.
    pub(crate) fn write_sys(&self, path: &[u8], value: &[u8]) -> Result<(), Errno> {
        let mut full = Vec::with_capacity(path.len() + 4);
        full.extend_from_slice(b"sys:");
        full.extend_from_slice(path);
        let handle = self
            .backend
            .open(&full, u32::from(raven_proto_fs::WIRE_OPEN_WRITE))?;
        let mut payload = value.to_vec();
        if !payload.ends_with(b"\n") {
            payload.push(b'\n');
        }
        let result = self.backend.write(handle, &payload).map(|_| ());
        let _ = self.backend.close(handle);
        result
    }

    /// The effective hostname (`sys:hostname`, else `uname.nodename`).
    pub fn hostname_value(&self) -> Vec<u8> {
        let sys_value = self.read_sys(b"hostname").ok();
        let nodename = self
            .backend
            .uname()
            .map(|uname| crate::sysinfo::uname::field_bytes(&uname.nodename).to_vec())
            .unwrap_or_default();
        crate::sysinfo::hostname::hostname_from(sys_value.as_deref(), &nodename)
    }

    /// The `sysinfo` record or the documented fallback.
    pub fn sysinfo(&self) -> crate::sysinfo::SysInfo {
        self.backend.sysinfo().unwrap_or(crate::sysinfo::FALLBACK)
    }

    /// `sysconf`.
    pub fn sysconf(&self, name: i32) -> Result<i64, Errno> {
        let info = self.sysinfo();
        let limits = self.rlimits.get();
        let inputs = crate::conf::SysconfInputs {
            cpus_online: i64::from(info.cpus_online),
            cpus_conf: i64::from(info.cpus_conf),
            phys_pages: info.mem_total_pages as i64,
            avphys_pages: info.mem_free_pages as i64,
            open_max: i64::from(limits.nofile_soft),
            child_max: 1024,
            stack_max: 8 * 1024 * 1024,
        };
        crate::conf::sysconf(name, &inputs)
    }

    /// `pathconf`: `stat` first so a missing path is `ENOENT` (musl ignores the path: wrong).
    pub fn pathconf(&self, path: &[u8], name: i32) -> Result<i64, Errno> {
        let stat = self.stat(path)?;
        let fs = crate::stat::statvfs_from_wire(&raven_proto_fs::StatFs {
            block_size: stat.st_blksize as u32,
            blocks: stat.st_blocks as u64,
            free_blocks: 0,
            files: 0,
            free_files: 0,
        });
        crate::conf::pathconf_value(name, Some(&fs))
    }

    /// `fpathconf`.
    pub fn fpathconf(&self, fd: i32, name: i32) -> Result<i64, Errno> {
        let fs = self.fstatvfs(fd)?;
        crate::conf::fpathconf_value(name, &fs)
    }

    /// `confstr`.
    pub fn confstr(&self, name: i32, buf: Option<&mut [u8]>) -> Result<usize, Errno> {
        crate::conf::confstr(name, buf)
    }

    /// `uname`.
    pub fn uname(&self) -> Result<raven_libc_abi::structs::Utsname, Errno> {
        crate::sysinfo::uname::uname(self)
    }

    /// `gethostname`.
    pub fn gethostname(&self, buf: &mut [u8]) -> Result<(), Errno> {
        crate::sysinfo::gethostname(self, buf)
    }

    /// `sethostname`.
    pub fn sethostname(&self, name: &[u8]) -> Result<(), Errno> {
        crate::sysinfo::sethostname(self, name)
    }

    /// `sysctlbyname`.
    pub fn sysctlbyname(
        &self,
        name: &[u8],
        old: Option<&mut [u8]>,
        new_value: Option<&[u8]>,
    ) -> Result<usize, Errno> {
        crate::sysinfo::sysctlbyname(self, name, old, new_value)
    }

    /// `sysctl`.
    pub fn sysctl(
        &self,
        mib: &[i32],
        old: Option<&mut [u8]>,
        new_value: Option<&[u8]>,
    ) -> Result<usize, Errno> {
        crate::sysinfo::sysctl(self, mib, old, new_value)
    }

    /// `sysctlnametomib`.
    pub fn sysctlnametomib(&self, name: &[u8], out: &mut [i32]) -> Result<usize, Errno> {
        crate::sysinfo::sysctlnametomib(name, out)
    }

    /// `getentropy` (COPY row 4).
    pub fn getentropy(&self, buf: &mut [u8]) -> Result<(), Errno> {
        crate::random::getentropy(self, buf)
    }

    /// A 32-byte seed for the hardening area.
    pub fn random_seed(&self) -> Result<[u8; 32], Errno> {
        crate::random::seed(self)
    }

    /// `lockf`; record locks are `ENOSYS` until PG-21.
    pub fn lockf(&self, fd: i32, function: i32, size: i64) -> Result<(), Errno> {
        let _ = self.get(fd)?;
        let mut record = crate::lock::lockf::UnsupportedLock { pid: 1 };
        crate::lock::lockf::lockf(fd, function, size, &mut record)
    }

    // ----------------------------------------------------------------- waiting

    /// The per-thread cached event queue (created once).
    pub(crate) fn event_queue(&self) -> Result<Handle, Errno> {
        if let Some(queue) = *self.event_queue.borrow() {
            return Ok(queue);
        }
        let queue = self.backend.evq_create()?;
        *self.event_queue.borrow_mut() = Some(queue);
        Ok(queue)
    }

    /// Drop a cached event queue and create a fresh one on the next wait.
    ///
    /// A forked child inherits the parent's queue handle. The kernel allows one waiter per queue,
    /// so the child must not keep using it; a lazy reset on `EBUSY` covers every fork/exec path
    /// without a fork hook in the shell.
    pub fn reset_event_queue(&self) {
        if let Some(queue) = self.event_queue.borrow_mut().take() {
            let _ = self.backend.handle_close(queue);
        }
    }

    /// Take a fresh scheme reference for every inherited scheme descriptor in the fork child.
    ///
    /// The kernel copies the handle table on fork, but the scheme server counts references per
    /// open file. A parent `close` must therefore not be the last reference while the child still
    /// holds the descriptor. `DUP` both gives the child its own handle and sends the server the
    /// duplicate notification; `closef`-style last-reference release then works on both sides.
    /// Sockets, tty/pty masters and pipes are scheme-backed: an sshd session forks after opening
    /// the accepted socket, the pty master and the privsep/channel pipes, and the parent's close
    /// must not release the server-side object while the session child still owns it.
    pub fn fork_child_inherit(&self) {
        let mut seen = alloc::collections::BTreeSet::new();
        let indices: Vec<u32> = self
            .fd
            .borrow()
            .open_entries()
            .into_iter()
            .map(|(_, entry)| entry.ofd)
            .collect();
        for index in indices {
            if !seen.insert(index) {
                continue;
            }
            let (kind, handle) = {
                let arena = self.ofd.borrow();
                let Some(ofd) = arena.get(index) else {
                    continue;
                };
                (ofd.kind, ofd.handle.get())
            };
            if !matches!(kind, OfdKind::Socket | OfdKind::Tty | OfdKind::Pipe)
                || handle == Handle::NONE
            {
                continue;
            }
            if let Ok(duplicate) = self
                .backend
                .dup(handle, raven_abi::types::Rights::ALL.bits())
                && let Some(ofd) = self.ofd.borrow_mut().get_mut(index)
            {
                ofd.handle.set(duplicate);
            }
        }
    }

    /// The OFD table epoch for `fd`, used in epoll cookies.
    pub(crate) fn ofd_epoch(&self, fd: i32) -> Option<u32> {
        let index = {
            let table = self.fd.borrow();
            let entry = table.entry(fd)?;
            if entry.is_free() {
                return None;
            }
            entry.ofd
        };
        self.ofd.borrow().get(index).map(|ofd| ofd.epoch)
    }

    /// The epoll registration table, shared by the epoll engine.
    pub(crate) fn epoll_state(&self) -> &RefCell<EpollState> {
        &self.epoll
    }
}

fn event_number(event: Handle) -> u32 {
    if event == Handle::NONE { 0 } else { event.0 }
}

fn is_tty_path(path: &[u8]) -> bool {
    matches!(path, b"/dev/tty" | b"/dev/console" | b"/dev/ptmx")
}

/// Map a public pty path to the pty server's scheme path.
///
/// `/dev/ptmx` is the clone device (FreeBSD `sys/kern/tty_pty.c` `ptmxopen`, NetBSD `ptyattach`)
/// and `/dev/pts/<N>` is the slave, exactly as the NetBSD/FreeBSD `pty:` node names.
fn pty_wire(path: &[u8]) -> Option<Vec<u8>> {
    if path == b"/dev/ptmx" {
        return Some(Vec::from(b"pty:ptmx" as &[u8]));
    }
    pty_slave_wire(path)
}

/// The `pty:` wire paths that name the master endpoint.
fn is_pty_master_wire(wire: &[u8]) -> bool {
    wire == b"pty:ptmx" || wire.starts_with(b"pty:master")
}

/// Map the public `/dev/pts/<N>` slave path to the pty server's `pty:slave/<N>` scheme path.
fn pty_slave_wire(path: &[u8]) -> Option<Vec<u8>> {
    let id = path.strip_prefix(b"/dev/pts/")?;
    if id.is_empty() {
        return None;
    }
    let mut wire = Vec::from(b"pty:slave/" as &[u8]);
    wire.extend_from_slice(id);
    Some(wire)
}

impl<B: SysBackend, L: Lock> OfdRef<'_, B, L> {
    fn snapshot_readable(&self) -> bool {
        let status = self.status();
        status & 0o3 != 0o1
    }

    fn snapshot_writable(&self) -> bool {
        self.status() & 0o3 != 0
    }
}

#[cfg(test)]
#[path = "p1c_ut.rs"]
mod p1c_tests;
