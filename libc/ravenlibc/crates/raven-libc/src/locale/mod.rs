// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<locale.h>`, `<langinfo.h>`, GNU gettext and XSI catalogs over `raven-libc-locale`.
//!
//! The process locale state and the intern table are shell statics; the shell is single-threaded
//! until C57, so the state is guarded by the one platform lock word. Environment lookups are the
//! C61 hand-over; `setlocale("")` therefore resolves through the empty environment rather than
//! inventing variables.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use raven_libc_abi::errno::Errno;
use raven_libc_abi::misc::{LC_ALL, LC_MESSAGES};
use raven_libc_abi::structs::Lconv;
use raven_libc_locale::gettext::Gettext;
use raven_libc_locale::locale::{EnvSource, Locale, LocaleState, NameTable};
use raven_libc_locale::{LConvData, localeconv as c_localeconv, nl_langinfo as c_langinfo};

use crate::ffi::platform;

const GLOBAL_SENTINEL: usize = usize::MAX;

struct EmptyEnv;

impl EnvSource for EmptyEnv {
    fn get(&self, _name: &[u8]) -> Option<&[u8]> {
        None
    }
}

struct LockedState {
    lock: AtomicBool,
    state: UnsafeCell<Option<LocaleState>>,
}

// SAFETY: the spin word serialises every access; the shell is single-threaded until C57.
unsafe impl Sync for LockedState {}

impl LockedState {
    const fn new() -> Self {
        Self {
            lock: AtomicBool::new(false),
            state: UnsafeCell::new(None),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut LocaleState) -> T) -> T {
        while self.lock.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        // SAFETY: the spin word gives exclusive access for the duration of the closure.
        let slot = unsafe { &mut *self.state.get() };
        if slot.is_none() {
            *slot = Some(LocaleState::new());
        }
        let result = match slot.as_mut() {
            Some(state) => f(state),
            None => {
                let mut temporary = LocaleState::new();
                f(&mut temporary)
            }
        };
        self.lock.store(false, Ordering::Release);
        result
    }
}

static STATE: LockedState = LockedState::new();
static CURRENT: AtomicPtr<c_void> = AtomicPtr::new(GLOBAL_SENTINEL as *mut c_void);
static LCONV: SyncCell<Lconv> = SyncCell(UnsafeCell::new(Lconv::ZERO));
static LOCALE_BUF: SyncCell<[u8; 512]> = SyncCell(UnsafeCell::new([0; 512]));
static RETURN_BUF: SyncCell<Vec<u8>> = SyncCell(UnsafeCell::new(Vec::new()));

struct SyncCell<T>(UnsafeCell<T>);

// SAFETY: these cells are process-global and only touched by the single-threaded shell; C57
// replaces them with per-thread state.
unsafe impl<T> Sync for SyncCell<T> {}

/// Copy `bytes` into the shell's one-C-string return buffer and return its pointer.
///
/// `gettext` and the domain setters return strings the caller must not free; C allows the next
/// call to overwrite them. A single process buffer is therefore the exact lifetime model. The
/// locale crate returns slices without a terminator, so the copy adds it.
fn c_string(bytes: &[u8]) -> *mut c_char {
    // SAFETY: the shell is single-threaded until C57; the buffer is only written here.
    let buffer = unsafe { &mut *RETURN_BUF.0.get() };
    buffer.clear();
    buffer.extend_from_slice(bytes);
    buffer.push(0);
    buffer.as_mut_ptr().cast::<c_char>()
}

fn current_locale() -> Locale {
    let pointer = CURRENT.load(Ordering::Acquire);
    if pointer as usize == GLOBAL_SENTINEL || pointer.is_null() {
        STATE.with(|state| state.global())
    } else {
        // SAFETY: every non-sentinel pointer in CURRENT came from `Box::into_raw` in
        // `newlocale`/`duplocale`, and `freelocale` clears it before dropping.
        unsafe { *(pointer.cast::<Locale>()) }
    }
}

fn current_table() -> &'static NameTable {
    STATE.with(|state| {
        // SAFETY: the table lives in STATE for the process lifetime; the pointer is only used
        // while the state outlives the returned borrow (the shell is process-global).
        unsafe { &*(state.table() as *const NameTable) }
    })
}

/// `setlocale`.
///
/// # Safety
/// `name` is NULL or a NUL-terminated locale name.
pub(crate) unsafe extern "C" fn setlocale_impl(cat: c_int, name: *const c_char) -> *mut c_char {
    if cat < 0 || cat > LC_ALL {
        return crate::ffi::ret::null(Errno(22));
    }
    let name = if name.is_null() {
        None
    } else {
        // SAFETY: the caller's C string contract.
        Some(unsafe { crate::ffi::cstr::slice(name) })
    };
    let result = STATE.with(|state| {
        state
            .setlocale(cat as usize, name, &EmptyEnv)
            .map(<[u8]>::to_vec)
    });
    match result {
        Some(bytes) => {
            // The returned pointer is valid until the next `setlocale`; the caller must not free
            // it. The shell buffer has room for the LC_ALL composite.
            // SAFETY: the shell is single-threaded and the buffer is only written here.
            let buffer = unsafe { &mut *LOCALE_BUF.0.get() };
            let copied = bytes.len().min(buffer.len().saturating_sub(1));
            if let (Some(dst), Some(src)) = (buffer.get_mut(..copied), bytes.get(..copied)) {
                dst.copy_from_slice(src);
            }
            if let Some(term) = buffer.get_mut(copied) {
                *term = 0;
            }
            buffer.as_mut_ptr().cast::<c_char>()
        }
        None => crate::ffi::ret::null(Errno(22)),
    }
}

/// `newlocale`.
///
/// # Safety
/// `name` is NUL terminated; `base` is NULL, `LC_GLOBAL_LOCALE` or a locale from this libc.
pub(crate) unsafe extern "C" fn newlocale_impl(
    mask: c_int,
    name: *const c_char,
    base: *mut c_void,
) -> *mut c_void {
    if name.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the name is NUL terminated.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    let base_locale = if base.is_null() {
        None
    } else if base as usize == GLOBAL_SENTINEL {
        Some(STATE.with(|state| state.global()))
    } else {
        // SAFETY: non-sentinel base objects are Box<Locale> from this module.
        Some(unsafe { *(base.cast::<Locale>()) })
    };
    let mask = u32::try_from(mask).unwrap_or(u32::MAX);
    let value = STATE.with(|state| state.newlocale(mask, name, base_locale));
    match value {
        Ok(locale) => Box::into_raw(Box::new(locale)).cast::<c_void>(),
        Err(_) => crate::ffi::ret::null(Errno(22)),
    }
}

/// `uselocale`.
///
/// # Safety
/// `locale` is `LC_GLOBAL_LOCALE`, NULL or a locale object.
pub(crate) unsafe extern "C" fn uselocale_impl(locale: *mut c_void) -> *mut c_void {
    let old = CURRENT.load(Ordering::Acquire);
    let new = if locale.is_null() || locale as usize == GLOBAL_SENTINEL {
        GLOBAL_SENTINEL as *mut c_void
    } else {
        locale
    };
    CURRENT.store(new, Ordering::Release);
    old
}

/// `duplocale`.
///
/// # Safety
/// `locale` is `LC_GLOBAL_LOCALE` or a locale object.
pub(crate) unsafe extern "C" fn duplocale_impl(locale: *mut c_void) -> *mut c_void {
    let value = if locale as usize == GLOBAL_SENTINEL {
        STATE.with(|state| state.global())
    } else if locale.is_null() {
        STATE.with(|state| state.global())
    } else {
        // SAFETY: non-sentinel objects are Box<Locale> from this module.
        unsafe { *(locale.cast::<Locale>()) }
    };
    Box::into_raw(Box::new(value)).cast::<c_void>()
}

/// `freelocale`.
///
/// # Safety
/// `locale` is a locale object from `newlocale`/`duplocale`, not `LC_GLOBAL_LOCALE`.
pub(crate) unsafe extern "C" fn freelocale_impl(locale: *mut c_void) {
    if locale.is_null() || locale as usize == GLOBAL_SENTINEL {
        return;
    }
    // SAFETY: the pointer came from `Box::into_raw` in this module.
    drop(unsafe { Box::from_raw(locale.cast::<Locale>()) });
}

/// `localeconv`.
pub(crate) extern "C" fn localeconv_impl() -> *mut Lconv {
    let data = c_localeconv();
    // SAFETY: the shell is single-threaded and the cell is only filled here.
    let target = unsafe { &mut *LCONV.0.get() };
    write_lconv(target, data);
    target
}

fn write_lconv(target: &mut Lconv, data: &'static LConvData) {
    let pointers = [
        data.decimal_point,
        data.thousands_sep,
        data.grouping,
        data.int_curr_symbol,
        data.currency_symbol,
        data.mon_decimal_point,
        data.mon_thousands_sep,
        data.mon_grouping,
        data.positive_sign,
        data.negative_sign,
    ];
    for (index, value) in pointers.iter().enumerate() {
        if let Some(slot) = target.bytes.get_mut(index) {
            *slot = value.as_ptr() as u64;
        }
    }
    let chars = [
        data.int_frac_digits,
        data.frac_digits,
        data.p_cs_precedes,
        data.p_sep_by_space,
        data.n_cs_precedes,
        data.n_sep_by_space,
        data.p_sign_posn,
        data.n_sign_posn,
        data.int_p_cs_precedes,
        data.int_p_sep_by_space,
        data.int_n_cs_precedes,
        data.int_n_sep_by_space,
        data.int_p_sign_posn,
        data.int_n_sign_posn,
    ];
    let raw = target.bytes.as_mut_ptr().cast::<u8>();
    for (index, value) in chars.iter().enumerate() {
        // SAFETY: `struct lconv` is 96 bytes, ten pointers followed by fourteen chars; the
        // index is bounded by the array and 80 + 14 <= 96.
        unsafe { *raw.add(80 + index) = *value as u8 };
    }
}

/// `nl_langinfo`.
pub(crate) extern "C" fn nl_langinfo_impl(item: u32) -> *mut c_char {
    let value = c_langinfo(item, &current_locale(), current_table());
    value.as_ptr().cast_mut().cast::<c_char>()
}

/// `nl_langinfo_l`.
///
/// # Safety
/// `locale` is a locale object.
pub(crate) unsafe extern "C" fn nl_langinfo_l_impl(item: u32, locale: *mut c_void) -> *mut c_char {
    if locale.is_null() || locale as usize == GLOBAL_SENTINEL {
        return nl_langinfo_impl(item);
    }
    // SAFETY: non-sentinel objects are Box<Locale> from this module.
    let loc = unsafe { *(locale.cast::<Locale>()) };
    let value = c_langinfo(item, &loc, current_table());
    value.as_ptr().cast_mut().cast::<c_char>()
}

struct ShellFiles;

impl raven_libc_locale::gettext::FileSource for ShellFiles {
    fn read_all(&self, path: &[u8], max: usize) -> Option<Vec<u8>> {
        let fd = platform::global()
            .open(path, raven_libc_abi::fcntl::O_RDONLY as u32, 0)
            .ok()?;
        let mut buffer = alloc::vec![0u8; max];
        let result = platform::global().read(fd, &mut buffer);
        let _ = platform::global().close(fd);
        let length = result.ok()?;
        buffer.truncate(length);
        Some(buffer)
    }
}

struct GettextCell(UnsafeCell<Option<Gettext>>);

// SAFETY: the shell is single-threaded until C57; the cell is only borrowed for one call.
unsafe impl Sync for GettextCell {}

static GETTEXT: GettextCell = GettextCell(UnsafeCell::new(None));

fn gettext_state() -> &'static mut Gettext {
    // SAFETY: single-threaded access as above; the Option is initialised once.
    let slot = unsafe { &mut *GETTEXT.0.get() };
    if slot.is_none() {
        *slot = Some(Gettext::new());
    }
    slot.as_mut().unwrap_or_else(|| unreachable_gettext())
}

fn unreachable_gettext() -> &'static mut Gettext {
    // SAFETY: `gettext_state` just wrote Some; this branch is unreachable.
    unsafe { core::hint::unreachable_unchecked() }
}

fn locale_name() -> Vec<u8> {
    let loc = current_locale();
    current_table().name(loc.cat[LC_MESSAGES as usize]).to_vec()
}

/// `gettext`.
///
/// # Safety
/// `msgid` is NUL terminated.
pub(crate) unsafe extern "C" fn gettext_impl(msgid: *const c_char) -> *mut c_char {
    if msgid.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the string is NUL terminated.
    let msgid = unsafe { crate::ffi::cstr::slice(msgid) };
    // SAFETY: forwarded C string contracts.
    unsafe { translate(None, msgid, msgid, 1) }
}

/// `dgettext`.
///
/// # Safety
/// `domain` and `msgid` are NUL terminated.
pub(crate) unsafe extern "C" fn dgettext_impl(
    domain: *const c_char,
    msgid: *const c_char,
) -> *mut c_char {
    if domain.is_null() || msgid.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: both strings are NUL terminated.
    let domain = unsafe { crate::ffi::cstr::slice(domain) };
    let msgid = unsafe { crate::ffi::cstr::slice(msgid) };
    // SAFETY: forwarded C string contracts.
    unsafe { translate(Some(domain), msgid, msgid, 1) }
}

/// `dcgettext`.
///
/// # Safety
/// `domain` and `msgid` are NUL terminated.
pub(crate) unsafe extern "C" fn dcgettext_impl(
    domain: *const c_char,
    msgid: *const c_char,
    _category: c_int,
) -> *mut c_char {
    // SAFETY: forwarded contracts.
    unsafe { dgettext_impl(domain, msgid) }
}

/// `ngettext`.
///
/// # Safety
/// `msgid1` and `msgid2` are NUL terminated.
pub(crate) unsafe extern "C" fn ngettext_impl(
    msgid1: *const c_char,
    msgid2: *const c_char,
    n: u64,
) -> *mut c_char {
    // SAFETY: forwarded C string contracts.
    unsafe { translate_pair(None, msgid1, msgid2, n) }
}

/// `dngettext`.
///
/// # Safety
/// `domain`, `msgid1` and `msgid2` are NUL terminated.
pub(crate) unsafe extern "C" fn dngettext_impl(
    domain: *const c_char,
    msgid1: *const c_char,
    msgid2: *const c_char,
    n: u64,
) -> *mut c_char {
    if domain.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the domain is NUL terminated.
    let domain = unsafe { crate::ffi::cstr::slice(domain) };
    // SAFETY: forwarded C string contracts.
    unsafe { translate_pair(Some(domain), msgid1, msgid2, n) }
}

/// `dcngettext`.
///
/// # Safety
/// `domain`, `msgid1` and `msgid2` are NUL terminated.
pub(crate) unsafe extern "C" fn dcngettext_impl(
    domain: *const c_char,
    msgid1: *const c_char,
    msgid2: *const c_char,
    n: u64,
    _category: c_int,
) -> *mut c_char {
    // SAFETY: forwarded contracts.
    unsafe { dngettext_impl(domain, msgid1, msgid2, n) }
}

unsafe fn translate(domain: Option<&[u8]>, msgid1: &[u8], msgid2: &[u8], n: u64) -> *mut c_char {
    let loc = current_locale();
    let result = gettext_state().dcngettext(
        &ShellFiles,
        &EmptyEnv,
        domain,
        msgid1,
        msgid2,
        n,
        LC_MESSAGES as usize,
        &loc,
        current_table(),
    );
    c_string(result)
}

unsafe fn translate_pair(
    domain: Option<&[u8]>,
    msgid1: *const c_char,
    msgid2: *const c_char,
    n: u64,
) -> *mut c_char {
    if msgid1.is_null() || msgid2.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: both strings are NUL terminated.
    let first = unsafe { crate::ffi::cstr::slice(msgid1) };
    let second = unsafe { crate::ffi::cstr::slice(msgid2) };
    // SAFETY: the same contracts.
    unsafe { translate(domain, first, second, n) }
}

/// `textdomain`.
///
/// # Safety
/// `domain` is NULL or NUL terminated.
pub(crate) unsafe extern "C" fn textdomain_impl(domain: *const c_char) -> *mut c_char {
    let value = if domain.is_null() {
        None
    } else {
        // SAFETY: the string is NUL terminated.
        Some(unsafe { crate::ffi::cstr::slice(domain) })
    };
    gettext_state()
        .textdomain(value)
        .map_or(ptr::null_mut(), c_string)
}

/// `bindtextdomain`.
///
/// # Safety
/// `domain` is NUL terminated; `dir` is NULL or NUL terminated.
pub(crate) unsafe extern "C" fn bindtextdomain_impl(
    domain: *const c_char,
    dir: *const c_char,
) -> *mut c_char {
    if domain.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the strings are NUL terminated.
    let domain = unsafe { crate::ffi::cstr::slice(domain) };
    let dir = if dir.is_null() {
        None
    } else {
        Some(unsafe { crate::ffi::cstr::slice(dir) })
    };
    gettext_state()
        .bindtextdomain(domain, dir)
        .map_or(ptr::null_mut(), c_string)
}

/// `bind_textdomain_codeset`.
///
/// # Safety
/// `domain` and `codeset` are NUL terminated.
pub(crate) unsafe extern "C" fn bind_textdomain_codeset_impl(
    domain: *const c_char,
    codeset: *const c_char,
) -> *mut c_char {
    if domain.is_null() || codeset.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the strings are NUL terminated.
    let domain = unsafe { crate::ffi::cstr::slice(domain) };
    let codeset = unsafe { crate::ffi::cstr::slice(codeset) };
    gettext_state()
        .bind_textdomain_codeset(domain, Some(codeset))
        .map_or(ptr::null_mut(), c_string)
}

/// Keep the locale name helper reachable for the future `catopen` binding.
#[allow(dead_code)]
fn _locale_name() -> Vec<u8> {
    locale_name()
}

mod bind_textdomain_codeset;
mod dngettext;
mod freelocale;
mod gettext;
mod setlocale;
