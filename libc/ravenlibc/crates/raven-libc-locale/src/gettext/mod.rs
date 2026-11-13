// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the gettext domain model and `dcngettext` from musl:src/locale/
// {dcngettext,textdomain,bind_textdomain_codeset,__lctrans}.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941.

//! GNU gettext lookups over a caller-supplied file source.
#![allow(clippy::indexing_slicing)] // RavenBSD: the index is bounded by the preceding size check.

use alloc::vec::Vec;

use crate::locale::{LC_MESSAGES, Locale, NameTable};

pub mod domain;
pub mod mo;
pub mod plural;

pub use domain::Domain;
pub use mo::mo_lookup;
pub use plural::{PLURAL_DEPTH, plural_eval};

/// Maximum `.mo` file size.
pub const MAX_MO_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_DIR: &[u8] = b"/usr/share/locale";

/// The file source used for `.mo` reads.
pub trait FileSource {
    /// Read `path` up to `max` bytes.
    fn read_all(&self, path: &[u8], max: usize) -> Option<Vec<u8>>;
}

/// The gettext domain state.
pub struct Gettext {
    domains: Vec<Domain>,
    current: Vec<u8>,
    codeset: Vec<u8>,
    cache: Vec<u8>,
}

impl Default for Gettext {
    fn default() -> Self {
        Self::new()
    }
}

impl Gettext {
    /// An empty state with the default domain `messages`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            domains: Vec::new(),
            current: b"messages".to_vec(),
            codeset: Vec::new(),
            cache: Vec::new(),
        }
    }

    /// `textdomain`.
    pub fn textdomain(&mut self, domain: Option<&[u8]>) -> Option<&[u8]> {
        if let Some(domain) = domain {
            self.current.clear();
            self.current.extend_from_slice(domain);
        }
        if self.current.is_empty() {
            None
        } else {
            Some(&self.current)
        }
    }

    /// `bindtextdomain`.
    pub fn bindtextdomain(&mut self, domain: &[u8], dir: Option<&[u8]>) -> Option<&[u8]> {
        if domain.is_empty() {
            return None;
        }
        if let Some(index) = self.domains.iter().position(|entry| entry.name == domain) {
            if let Some(dir) = dir
                && let Some(entry) = self.domains.get_mut(index)
            {
                entry.dir = Some(dir.to_vec());
            }
            return self
                .domains
                .get(index)
                .and_then(|entry| entry.dir.as_deref());
        }
        let entry = Domain {
            name: domain.to_vec(),
            dir: dir.map(<[u8]>::to_vec),
        };
        self.domains.push(entry);
        self.domains.last().and_then(|entry| entry.dir.as_deref())
    }

    /// `bind_textdomain_codeset`; UTF-8 is the only codeset, the call is accepted.
    pub fn bind_textdomain_codeset(
        &mut self,
        domain: &[u8],
        codeset: Option<&[u8]>,
    ) -> Option<&[u8]> {
        if domain.is_empty() {
            return None;
        }
        if let Some(codeset) = codeset {
            self.codeset.clear();
            self.codeset.extend_from_slice(codeset);
        }
        if self.codeset.is_empty() {
            None
        } else {
            Some(&self.codeset)
        }
    }

    /// `dcngettext`.
    #[allow(clippy::too_many_arguments)] // The C signature has the same ten arguments.
    pub fn dcngettext<'a>(
        &'a mut self,
        fs: &dyn FileSource,
        env: &dyn crate::locale::EnvSource,
        domain: Option<&[u8]>,
        msgid1: &'a [u8],
        msgid2: &'a [u8],
        n: u64,
        cat: usize,
        loc: &Locale,
        table: &NameTable,
    ) -> &'a [u8] {
        let fallback = if n == 1 { msgid1 } else { msgid2 };
        if cat != LC_MESSAGES {
            return fallback;
        }
        let name = table.name(loc.cat[cat]);
        if name == b"C" || name == b"POSIX" || name == b"C.UTF-8" {
            return fallback;
        }
        let domain = domain
            .filter(|value| !value.is_empty())
            .unwrap_or(&self.current);
        if domain.is_empty() {
            return fallback;
        }
        let dir = self
            .domains
            .iter()
            .find(|entry| entry.name == domain)
            .and_then(|entry| entry.dir.as_deref())
            .unwrap_or(DEFAULT_DIR);
        let mut candidates = alloc::vec::Vec::new();
        if let Some(language) = env.get(b"LANGUAGE") {
            for part in language
                .split(|byte| *byte == b':')
                .filter(|part| !part.is_empty())
            {
                candidates.push(part.to_vec());
            }
        }
        candidates.push(name.to_vec());
        if let Some(dot) = name.iter().position(|byte| *byte == b'.') {
            if let Some(short) = name.get(..dot) {
                candidates.push(short.to_vec());
                if let Some(underscore) = short.iter().position(|byte| *byte == b'_')
                    && let Some(lang) = short.get(..underscore)
                {
                    candidates.push(lang.to_vec());
                }
            }
        } else if let Some(underscore) = name.iter().position(|byte| *byte == b'_')
            && let Some(lang) = name.get(..underscore)
        {
            candidates.push(lang.to_vec());
        }
        for candidate in candidates {
            let mut path = Vec::new();
            path.extend_from_slice(dir);
            path.push(b'/');
            path.extend_from_slice(&candidate);
            path.extend_from_slice(b"/LC_MESSAGES/");
            path.extend_from_slice(domain);
            path.extend_from_slice(b".mo");
            let Some(mo) = fs.read_all(&path, MAX_MO_BYTES) else {
                continue;
            };
            let Some(raw) = mo_lookup(&mo, msgid1) else {
                continue;
            };
            let forms: Vec<&[u8]> = raw.split(|byte| *byte == 0).collect();
            let index = plural_index(&mo, n, forms.len());
            let Some(translation) = forms.get(index).copied().or_else(|| forms.first().copied())
            else {
                continue;
            };
            self.cache.clear();
            self.cache.extend_from_slice(translation);
            return self.cache.as_slice();
        }
        fallback
    }
}

fn plural_index(mo: &[u8], n: u64, forms: usize) -> usize {
    if forms <= 1 {
        return 0;
    }
    let Some(header) = mo_lookup(mo, b"") else {
        return if n == 1 { 0 } else { 1.min(forms - 1) };
    };
    let Some(expression) = header
        .windows(13)
        .position(|window| window == b"Plural-Forms:")
    else {
        return if n == 1 { 0 } else { 1.min(forms - 1) };
    };
    let rest = header.get(expression..).unwrap_or(b"");
    let Some(plural) = rest.windows(7).position(|window| window == b"plural=") else {
        return if n == 1 { 0 } else { 1.min(forms - 1) };
    };
    let expr = rest.get(plural + 7..).unwrap_or(b"");
    let end = expr
        .iter()
        .position(|byte| *byte == b';' || *byte == b'\n')
        .unwrap_or(expr.len());
    let expr = expr.get(..end).unwrap_or(b"");
    let Some(value) = plural_eval(expr, n) else {
        return if n == 1 { 0 } else { 1.min(forms - 1) };
    };
    usize::try_from(value).unwrap_or(0).min(forms - 1)
}

#[cfg(test)]
#[path = "gettext_ut.rs"]
mod tests;
