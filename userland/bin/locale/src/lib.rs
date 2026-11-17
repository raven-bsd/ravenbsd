// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Locale reporting over the existing `raven-libc-locale` state model.

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_libc_locale::{
    CODESET, EnvSource, LC_ALL, LC_COLLATE, LC_CTYPE, LC_MESSAGES, LC_MONETARY, LC_NUMERIC,
    LC_TIME, LocaleState, nl_langinfo,
};

/// One POSIX locale category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Category {
    /// Environment and output name.
    pub name: &'static str,
    /// `LC_*` index in the locale state.
    pub index: usize,
}

/// The six categories reported by `locale`.
pub const CATEGORIES: [Category; 6] = [
    Category {
        name: "LC_CTYPE",
        index: LC_CTYPE,
    },
    Category {
        name: "LC_NUMERIC",
        index: LC_NUMERIC,
    },
    Category {
        name: "LC_TIME",
        index: LC_TIME,
    },
    Category {
        name: "LC_COLLATE",
        index: LC_COLLATE,
    },
    Category {
        name: "LC_MONETARY",
        index: LC_MONETARY,
    },
    Category {
        name: "LC_MESSAGES",
        index: LC_MESSAGES,
    },
];

/// Locales this build knows by name.
pub const LOCALES: [&str; 3] = ["C", "POSIX", "C.UTF-8"];
/// Charmaps this build knows by name.
pub const CHARMAPS: [&str; 2] = ["ANSI_X3.4-1968", "UTF-8"];
/// Keywords reported by `locale -k`.
pub const KEYWORDS: [&str; 9] = [
    "LANG",
    "LC_CTYPE",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_COLLATE",
    "LC_MONETARY",
    "LC_MESSAGES",
    "LC_ALL",
    "codeset",
];

struct Empty;

impl EnvSource for Empty {
    fn get(&self, _name: &[u8]) -> Option<&[u8]> {
        None
    }
}

/// Resolve the categories from `env`, as `setlocale(LC_ALL, "")` does.
#[must_use]
pub fn resolve(env: &dyn EnvSource) -> LocaleState {
    let mut state = LocaleState::new();
    state.setlocale(LC_ALL, Some(b""), env);
    state
}

/// One category name, or the composite `LC_ALL` value.
#[must_use]
pub fn category_value(state: &mut LocaleState, index: usize) -> String {
    state
        .setlocale(index, None, &Empty)
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .unwrap_or_default()
}

/// The `codeset` value from `nl_langinfo(CODESET)`.
#[must_use]
pub fn codeset(state: &LocaleState) -> String {
    let value = nl_langinfo(CODESET, &state.global(), state.table());
    let end = value
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(value.len());
    String::from_utf8_lossy(&value[..end]).into_owned()
}

fn env_value(env: &dyn EnvSource, name: &[u8]) -> String {
    env.get(name)
        .map(|value| String::from_utf8_lossy(value).into_owned())
        .unwrap_or_default()
}

/// Render the default `locale` report.
#[must_use]
pub fn render(env: &dyn EnvSource) -> String {
    let mut state = resolve(env);
    let mut output = format!("LANG={}\n", env_value(env, b"LANG"));
    for category in CATEGORIES {
        output.push_str(&format!(
            "{}=\"{}\"\n",
            category.name,
            category_value(&mut state, category.index)
        ));
    }
    output.push_str(&format!("LC_ALL={}\n", env_value(env, b"LC_ALL")));
    output.push_str(&format!("codeset={}\n", codeset(&state)));
    output
}

/// Render one named category, or the raw `LANG`/resolved `LC_ALL`.
pub fn query(state: &mut LocaleState, env: &dyn EnvSource, name: &str) -> Result<String, String> {
    if name == "LANG" {
        return Ok(format!("LANG={}", env_value(env, b"LANG")));
    }
    if name == "LC_ALL" {
        return Ok(format!("LC_ALL=\"{}\"", category_value(state, LC_ALL)));
    }
    if let Some(category) = CATEGORIES.iter().find(|category| category.name == name) {
        return Ok(format!(
            "{}=\"{}\"",
            name,
            category_value(state, category.index)
        ));
    }
    Err(format!("unknown locale category `{name}`"))
}

/// List the known names for `-a` or `-m`.
#[must_use]
pub fn list(values: &[&'static str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
