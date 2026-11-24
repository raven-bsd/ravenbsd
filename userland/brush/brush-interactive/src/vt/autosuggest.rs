// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

//! History autosuggestions for the Vt editor.

use super::line_editor::History;

/// Returns the newest history suffix for `text`, unless it is empty or the entry
/// is the same line.
///
/// Multi-line history entries are skipped: the hint is a one-line suffix.
pub(crate) fn suffix(text: &str, history: &(impl History + ?Sized)) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    for index in (0..history.len()).rev() {
        let Some(entry) = history.get(index) else {
            continue;
        };
        if entry.len() > text.len()
            && entry.starts_with(text)
            && !entry.contains('\n')
            && !entry.contains('\r')
        {
            return Some(entry[text.len()..].to_owned());
        }
    }
    None
}

#[cfg(test)]
#[path = "autosuggest_ut.rs"]
mod tests;
