// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the locale state model and `setlocale` from musl:src/locale/setlocale.c,
// locale_map.c, newlocale.c, uselocale.c and duplocale.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941 (no per-file header; covered by musl/COPYRIGHT).

//! The locale state, the interned name table and `setlocale`/`newlocale`/`uselocale`.
#![allow(clippy::indexing_slicing)] // RavenBSD: the index is bounded by the preceding size check.

use alloc::vec::Vec;

/// `LC_CTYPE`.
pub const LC_CTYPE: usize = 0;
/// `LC_NUMERIC`.
pub const LC_NUMERIC: usize = 1;
/// `LC_TIME`.
pub const LC_TIME: usize = 2;
/// `LC_COLLATE`.
pub const LC_COLLATE: usize = 3;
/// `LC_MONETARY`.
pub const LC_MONETARY: usize = 4;
/// `LC_MESSAGES`.
pub const LC_MESSAGES: usize = 5;
/// `LC_ALL`.
pub const LC_ALL: usize = 6;
/// The category mask with every category bit set.
pub const LC_ALL_MASK: u32 = 0x7fff_ffff;
/// Maximum stored name length.
pub const LOCALE_NAME_MAX: usize = 63;
/// The intern table bound; the 257th distinct name maps to `C.UTF-8`.
pub const MAX_INTERNED: usize = 256;

const C_NAME: &[u8] = b"C";
const CUTF8_NAME: &[u8] = b"C.UTF-8";

/// An interned locale name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NameId(u16);

#[derive(Clone, Copy)]
struct LocName {
    name: [u8; LOCALE_NAME_MAX + 1],
    len: u8,
    utf8: bool,
}

impl LocName {
    fn new(name: &[u8], utf8: bool) -> Self {
        let len = name.len().min(LOCALE_NAME_MAX);
        let mut bytes = [0u8; LOCALE_NAME_MAX + 1];
        if let Some(dst) = bytes.get_mut(..len)
            && let Some(src) = name.get(..len)
        {
            dst.copy_from_slice(src);
        }
        Self {
            name: bytes,
            len: len as u8,
            utf8,
        }
    }

    fn bytes(&self) -> &[u8] {
        self.name.get(..usize::from(self.len)).unwrap_or(b"")
    }
}

/// The process-local intern table of locale names.
pub struct NameTable {
    names: Vec<LocName>,
}

impl Default for NameTable {
    fn default() -> Self {
        Self::new()
    }
}

impl NameTable {
    /// A table holding the fixed `C` (id 0) and `C.UTF-8` (id 1) entries.
    #[must_use]
    pub fn new() -> Self {
        Self {
            names: alloc::vec![LocName::new(C_NAME, false), LocName::new(CUTF8_NAME, true),],
        }
    }

    /// Intern `name`; a full table maps the request to `C.UTF-8`.
    pub fn intern(&mut self, name: &[u8]) -> NameId {
        if let Some(index) = self.names.iter().position(|entry| entry.bytes() == name) {
            return NameId(index as u16);
        }
        if self.names.len() >= MAX_INTERNED {
            return NameId(1);
        }
        let utf8 = !(name == C_NAME || name == b"POSIX");
        self.names.push(LocName::new(name, utf8));
        NameId((self.names.len() - 1) as u16)
    }

    /// The stored name bytes for `id`.
    #[must_use]
    pub fn name(&self, id: NameId) -> &[u8] {
        self.names
            .get(usize::from(id.0))
            .map_or(b"", LocName::bytes)
    }

    /// Whether `id` selects the UTF-8 multibyte mode.
    #[must_use]
    pub fn is_utf8(&self, id: NameId) -> bool {
        self.names
            .get(usize::from(id.0))
            .is_some_and(|entry| entry.utf8)
    }
}

/// A `locale_t` object: one interned name per category.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Locale {
    /// The six category names.
    pub cat: [NameId; 6],
}

impl Locale {
    /// The C locale (`C` in every category).
    pub const C: Self = Self {
        cat: [NameId(0); 6],
    };
    /// The UTF-8 C locale (`C.UTF-8` in every category).
    pub const CUTF8: Self = Self {
        cat: [NameId(1); 6],
    };

    /// Whether `LC_CTYPE` selects UTF-8.
    #[must_use]
    pub fn ctype_is_utf8(&self, table: &NameTable) -> bool {
        table.is_utf8(self.cat[LC_CTYPE])
    }

    /// `MB_CUR_MAX`: 1 for C/POSIX, 4 for every UTF-8 locale.
    #[must_use]
    pub fn mb_cur_max(&self, table: &NameTable) -> usize {
        if self.ctype_is_utf8(table) { 4 } else { 1 }
    }
}

/// The environment view the caller supplies.
pub trait EnvSource {
    /// Return the value of `name`, or `None`.
    fn get(&self, name: &[u8]) -> Option<&[u8]>;
}

/// A `locale_t` argument: the global object or a real object.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LocaleArg {
    /// `LC_GLOBAL_LOCALE`.
    Global,
    /// A caller-owned locale object.
    Obj(Locale),
}

/// The locale failure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LocaleError {
    /// `EINVAL`.
    Invalid,
}

/// The process-global locale state.
pub struct LocaleState {
    global: Locale,
    table: NameTable,
    all_buf: [u8; LC_ALL * (LOCALE_NAME_MAX + 1)],
}

impl Default for LocaleState {
    fn default() -> Self {
        Self::new()
    }
}

impl LocaleState {
    /// A state starting in the C locale.
    #[must_use]
    pub fn new() -> Self {
        Self {
            global: Locale::C,
            table: NameTable::new(),
            all_buf: [0; LC_ALL * (LOCALE_NAME_MAX + 1)],
        }
    }

    /// The current global locale.
    #[must_use]
    pub const fn global(&self) -> Locale {
        self.global
    }

    /// The intern table.
    #[must_use]
    pub const fn table(&self) -> &NameTable {
        &self.table
    }

    /// `setlocale`; `name == None` queries.
    pub fn setlocale(
        &mut self,
        cat: usize,
        name: Option<&[u8]>,
        env: &dyn EnvSource,
    ) -> Option<&[u8]> {
        if cat > LC_ALL {
            return None;
        }
        if cat == LC_ALL {
            if let Some(name) = name {
                let mut next = Locale::CUTF8;
                if name.contains(&b';') {
                    let mut parts = name.split(|byte| *byte == b';');
                    for index in 0..LC_ALL {
                        let part = parts.next().filter(|part| part.len() <= LOCALE_NAME_MAX);
                        let value = match part {
                            Some(part) => self.resolve(index, part, env),
                            None => self.resolve(index, CUTF8_NAME, env),
                        };
                        next.cat[index] = value;
                    }
                } else {
                    for index in 0..LC_ALL {
                        next.cat[index] = self.resolve(index, name, env);
                    }
                }
                self.global = next;
            }
            return self.format_all();
        }
        if let Some(name) = name {
            let value = self.resolve(cat, name, env);
            self.global.cat[cat] = value;
        }
        Some(self.table.name(self.global.cat[cat]))
    }

    /// `newlocale`: overwrite the categories in `mask`.
    pub fn newlocale(
        &mut self,
        mask: u32,
        name: &[u8],
        base: Option<Locale>,
    ) -> Result<Locale, LocaleError> {
        if name.is_empty() || mask & !((1 << LC_ALL) - 1) != 0 {
            return Err(LocaleError::Invalid);
        }
        let mut locale = base.unwrap_or(Locale::C);
        for index in 0..LC_ALL {
            if mask & (1 << index) != 0 {
                locale.cat[index] = self.resolve(index, name, &NoEnv);
            }
        }
        Ok(locale)
    }

    /// `duplocale`.
    #[must_use]
    pub fn duplocale(&self, src: LocaleArg) -> Locale {
        match src {
            LocaleArg::Global => self.global,
            LocaleArg::Obj(locale) => locale,
        }
    }

    fn format_all(&mut self) -> Option<&[u8]> {
        let first = self.global.cat[0];
        let same = self.global.cat.iter().all(|id| *id == first);
        if same {
            return Some(self.table.name(first));
        }
        let mut out = alloc::vec::Vec::new();
        for index in 0..LC_ALL {
            if index != 0 {
                out.push(b';');
            }
            out.extend_from_slice(self.table.name(self.global.cat[index]));
        }
        let len = out.len().min(self.all_buf.len());
        if let Some(dst) = self.all_buf.get_mut(..len)
            && let Some(src) = out.get(..len)
        {
            dst.copy_from_slice(src);
        }
        Some(self.all_buf.get(..len).unwrap_or(b""))
    }

    fn resolve(&mut self, cat: usize, name: &[u8], env: &dyn EnvSource) -> NameId {
        let mut value = name;
        if value.is_empty() {
            value = self.env_value(cat, env);
        }
        if value.is_empty()
            || value.first() == Some(&b'.')
            || value.contains(&b'/')
            || value.len() > LOCALE_NAME_MAX
        {
            value = CUTF8_NAME;
        }
        if value == C_NAME || value == b"POSIX" {
            return self.table.intern(value);
        }
        self.table.intern(value)
    }

    fn env_value<'a>(&self, cat: usize, env: &'a dyn EnvSource) -> &'a [u8] {
        let names: [&[u8]; 7] = [
            b"LC_ALL",
            b"LC_CTYPE",
            b"LC_NUMERIC",
            b"LC_TIME",
            b"LC_COLLATE",
            b"LC_MONETARY",
            b"LC_MESSAGES",
        ];
        if let Some(value) = env.get(b"LC_ALL").filter(|value| !value.is_empty()) {
            return value;
        }
        let index = cat.saturating_add(1).min(names.len() - 1);
        if let Some(value) = env
            .get(names.get(index).copied().unwrap_or(b"LC_CTYPE"))
            .filter(|value| !value.is_empty())
        {
            return value;
        }
        env.get(b"LANG")
            .filter(|value| !value.is_empty())
            .unwrap_or(CUTF8_NAME)
    }
}

struct NoEnv;

impl EnvSource for NoEnv {
    fn get(&self, _name: &[u8]) -> Option<&[u8]> {
        None
    }
}

/// `uselocale`; `slot` is the caller's per-thread locale field.
pub fn uselocale(slot: &mut LocaleArg, new: Option<LocaleArg>) -> LocaleArg {
    let old = *slot;
    if let Some(new) = new {
        *slot = new;
    }
    old
}

#[cfg(test)]
#[path = "locale_ut.rs"]
mod tests;
