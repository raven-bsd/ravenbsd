// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The TUI state machine: searchable module list and the form pane.

use std::path::{Path, PathBuf};

use crate::model::{self, Field, FieldKind, Module};

/// A terminal-independent input event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A printable character.
    Char(char),
    /// Enter/Return.
    Enter,
    /// Escape.
    Escape,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Tab.
    Tab,
    /// Backspace/delete.
    Backspace,
    /// Ctrl-S.
    CtrlSave,
    /// Ctrl-C.
    CtrlCancel,
}

/// The action the run loop should take after an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Redraw and keep going.
    Continue,
    /// The plan was written.
    Saved(PathBuf),
    /// The dry-run plan was produced and nothing was written.
    DryRun(String),
    /// The user cancelled.
    Cancelled,
    /// Validation failed; keep the TUI open with field errors.
    Validation(Vec<String>),
    /// A write or internal error.
    Error(String),
}

/// Which pane has focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// Searchable module list.
    List,
    /// Form fields.
    Form,
}

/// TUI options from the CLI flags and terminal state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Render the plan without writing the drop-in.
    pub dry_run: bool,
    /// Use ANSI styling.
    pub color: bool,
    /// Layout width in columns.
    pub width: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            dry_run: false,
            color: false,
            width: 100,
        }
    }
}

/// The TUI state.
pub struct Tui {
    pub(crate) root: PathBuf,
    pub(crate) options: Options,
    pub(crate) modules: Vec<Module>,
    pub(crate) active: usize,
    pub(crate) selected: usize,
    pub(crate) filter: String,
    pub(crate) search: bool,
    pub(crate) focus: Focus,
    pub(crate) field: usize,
    pub(crate) status: String,
    notice: Option<String>,
}

impl Tui {
    /// Build the TUI for one configuration root. A bad existing drop-in is a status warning, not
    /// a failure to open.
    #[must_use]
    pub fn new(root: &Path, options: Options) -> Self {
        let (current, warning) = model::load_network(root);
        let status = warning.unwrap_or_else(|| {
            String::from("Type / or a letter to filter; Enter opens the module")
        });
        Self {
            root: root.to_path_buf(),
            options,
            modules: model::modules(&current),
            active: 0,
            selected: 0,
            filter: String::new(),
            search: false,
            focus: Focus::List,
            field: 0,
            status,
            notice: None,
        }
    }

    /// Take a status line to print after this event, if any.
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// Module indices matching the current filter, in registry order.
    #[must_use]
    pub fn filtered_modules(&self) -> Vec<usize> {
        let needle = self.filter.to_ascii_lowercase();
        self.modules
            .iter()
            .enumerate()
            .filter(|(_, module)| {
                needle.is_empty()
                    || module.title.to_ascii_lowercase().contains(&needle)
                    || module.summary.to_ascii_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// The active module.
    #[must_use]
    pub fn selected_module(&self) -> Option<&Module> {
        self.modules.get(self.active)
    }

    /// The active field.
    #[must_use]
    pub fn selected_field(&self) -> Option<&Field> {
        self.modules
            .get(self.active)
            .and_then(|module| module.fields.get(self.field))
    }

    /// Handle one input event.
    pub fn handle(&mut self, event: Event) -> Action {
        match self.focus {
            Focus::List => self.handle_list(event),
            Focus::Form => self.handle_form(event),
        }
    }

    fn handle_list(&mut self, event: Event) -> Action {
        match event {
            Event::Char('/') => {
                self.search = true;
                self.filter.clear();
                self.selected = 0;
                self.status = String::from("Search: ");
                Action::Continue
            }
            Event::Char('q') if !self.search => Action::Cancelled,
            Event::Char(character)
                if !self.search || character.is_ascii_graphic() || character == ' ' =>
            {
                self.search = true;
                self.filter.push(character);
                self.selected = 0;
                self.status = format!("Search: {}", self.filter);
                Action::Continue
            }
            Event::Backspace if self.search => {
                self.filter.pop();
                self.selected = 0;
                self.status = format!("Search: {}", self.filter);
                Action::Continue
            }
            Event::Escape if self.search => {
                self.search = false;
                self.filter.clear();
                self.selected = 0;
                self.status = String::from("Filter cleared");
                Action::Continue
            }
            Event::Escape => Action::Cancelled,
            Event::Up => {
                self.move_selection(false);
                Action::Continue
            }
            Event::Down => {
                self.move_selection(true);
                Action::Continue
            }
            Event::Enter | Event::Tab => {
                if let Some(index) = self.filtered_modules().get(self.selected).copied() {
                    self.active = index;
                    self.focus = Focus::Form;
                    self.field = 0;
                    let title = self.modules[index].title;
                    self.status = format!("Module: {title}");
                    self.notice = Some(format!("raven config: module={title}"));
                }
                Action::Continue
            }
            _ => Action::Continue,
        }
    }

    fn handle_form(&mut self, event: Event) -> Action {
        match event {
            Event::Escape => {
                self.focus = Focus::List;
                self.status = String::from("Module list");
                Action::Continue
            }
            Event::CtrlCancel => Action::Cancelled,
            Event::CtrlSave => self.save(),
            Event::Enter => {
                if self.selected_module().is_some_and(|module| module.writable) {
                    self.save()
                } else {
                    self.focus = Focus::List;
                    self.status = String::from("Read-only module");
                    Action::Continue
                }
            }
            Event::Up => {
                self.move_field(false);
                Action::Continue
            }
            Event::Down | Event::Tab => {
                self.move_field(true);
                Action::Continue
            }
            Event::Left => {
                self.cycle_choice(false);
                Action::Continue
            }
            Event::Right => {
                self.cycle_choice(true);
                Action::Continue
            }
            Event::Backspace => {
                if self.selected_module().is_some_and(|module| module.writable)
                    && let Some(field) = self
                        .modules
                        .get_mut(self.active)
                        .and_then(|module| module.fields.get_mut(self.field))
                    && field.kind == FieldKind::Text
                {
                    field.value.pop();
                    field.error = None;
                }
                Action::Continue
            }
            Event::Char(character) => {
                if self.selected_module().is_some_and(|module| module.writable)
                    && let Some(field) = self
                        .modules
                        .get_mut(self.active)
                        .and_then(|module| module.fields.get_mut(self.field))
                {
                    match field.kind {
                        FieldKind::Text if character.is_ascii_graphic() || character == ' ' => {
                            field.value.push(character);
                            field.error = None;
                        }
                        FieldKind::Choice(choices) if character == 'd' || character == 's' => {
                            let wanted = if character == 'd' { "dhcp" } else { "static" };
                            if choices.contains(&wanted) {
                                field.value = wanted.to_string();
                                field.error = None;
                            }
                        }
                        _ => {}
                    }
                }
                Action::Continue
            }
        }
    }

    fn move_selection(&mut self, down: bool) {
        let count = self.filtered_modules().len();
        if count == 0 {
            self.selected = 0;
        } else if down {
            self.selected = (self.selected + 1) % count;
        } else {
            self.selected = if self.selected == 0 {
                count - 1
            } else {
                self.selected - 1
            };
        }
    }

    fn move_field(&mut self, down: bool) {
        let count = self
            .modules
            .get(self.active)
            .map_or(0, |module| module.fields.len());
        if count == 0 {
            self.field = 0;
        } else if down {
            self.field = (self.field + 1) % count;
        } else {
            self.field = if self.field == 0 {
                count - 1
            } else {
                self.field - 1
            };
        }
    }

    fn cycle_choice(&mut self, forward: bool) {
        let Some(choices) = self
            .modules
            .get(self.active)
            .and_then(|module| module.fields.get(self.field))
            .and_then(|field| match field.kind {
                FieldKind::Choice(choices) => Some(choices),
                _ => None,
            })
        else {
            return;
        };
        let Some(field) = self
            .modules
            .get_mut(self.active)
            .and_then(|module| module.fields.get_mut(self.field))
        else {
            return;
        };
        let current = if field.value.is_empty() {
            field.current.as_str()
        } else {
            field.value.as_str()
        };
        let index = choices
            .iter()
            .position(|choice| *choice == current)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % choices.len()
        } else if index == 0 {
            choices.len() - 1
        } else {
            index - 1
        };
        field.value = choices[next].to_string();
        field.error = None;
    }

    fn save(&mut self) -> Action {
        let Some(module) = self.modules.get(self.active) else {
            return Action::Error(String::from("no active module"));
        };
        if !module.writable {
            self.status = String::from("Read-only module; no save");
            return Action::Continue;
        }
        let fields = module.fields.clone();
        match model::plan_from_fields(&fields) {
            Err(errors) => {
                for field in &mut self.modules[self.active].fields {
                    field.error = None;
                }
                let mut messages = Vec::with_capacity(errors.len());
                for error in errors {
                    messages.push(error.message.clone());
                    if let Some(field) = self.modules[self.active]
                        .fields
                        .iter_mut()
                        .find(|field| field.key == error.key)
                    {
                        field.error = Some(error.message);
                    }
                }
                self.status = format!("Validation failed: {} field(s)", messages.len());
                self.notice = Some(String::from("raven config: validation failed"));
                Action::Validation(messages)
            }
            Ok(plan) => {
                if self.options.dry_run {
                    self.status = String::from("Dry run: nothing written");
                    Action::DryRun(plan.to_toml())
                } else {
                    match model::write_atomic(&self.root, &plan) {
                        Ok(path) => Action::Saved(path),
                        Err(error) => Action::Error(error.to_string()),
                    }
                }
            }
        }
    }
}
