// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

//! Exports traits for shell interfaces implemented by callers.

mod keybindings;

pub use keybindings::{InputFunction, Key, KeyAction, KeyBindings, KeySequence, KeyStroke};
