// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! State-machine tests for search, selection, editing, validation and save.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Action, Event, Focus, Options, Tui};
use crate::model::network_path;

static NEXT: AtomicUsize = AtomicUsize::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "raven-tui-app-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn feed(tui: &mut Tui, events: &[Event]) -> Option<Action> {
    let mut last = None;
    for event in events {
        last = Some(tui.handle(*event));
    }
    last
}

fn text_events(text: &str) -> Vec<Event> {
    text.chars().map(Event::Char).collect()
}

#[test]
fn search_filters_and_selects_services() {
    let root = TempDir::new();
    let mut tui = Tui::new(root.path(), Options::default());
    feed(&mut tui, &[Event::Char('/')]);
    feed(&mut tui, &text_events("ser"));
    let action = tui.handle(Event::Enter);
    assert_eq!(action, Action::Continue);
    assert_eq!(tui.focus, Focus::Form);
    assert_eq!(tui.selected_module().unwrap().title, "Services");
    assert_eq!(
        tui.take_notice().as_deref(),
        Some("raven config: module=Services")
    );
}

#[test]
fn typing_without_slash_filters() {
    let root = TempDir::new();
    let mut tui = Tui::new(root.path(), Options::default());
    feed(&mut tui, &text_events("the"));
    tui.handle(Event::Enter);
    assert_eq!(tui.selected_module().unwrap().title, "Theme");
}

#[test]
fn enter_selects_network_and_ctrl_s_saves_dhcp() {
    let root = TempDir::new();
    let mut tui = Tui::new(root.path(), Options::default());
    assert_eq!(tui.handle(Event::Enter), Action::Continue);
    assert_eq!(tui.selected_module().unwrap().title, "Network");
    feed(&mut tui, &text_events("raven-tui"));
    let action = tui.handle(Event::CtrlSave);
    assert!(matches!(action, Action::Saved(_)));
    let text = fs::read_to_string(network_path(root.path())).unwrap();
    assert!(text.contains("hostname = \"raven-tui\""));
    assert!(text.contains("mode = \"dhcp\""));
}

#[test]
fn dry_run_produces_a_plan_and_writes_nothing() {
    let root = TempDir::new();
    let options = Options {
        dry_run: true,
        ..Options::default()
    };
    let mut tui = Tui::new(root.path(), options);
    tui.handle(Event::Enter);
    feed(&mut tui, &text_events("dry"));
    let action = tui.handle(Event::CtrlSave);
    let Action::DryRun(text) = action else {
        panic!("expected DryRun");
    };
    assert!(text.contains("hostname = \"dry\""));
    assert!(!network_path(root.path()).exists());
}

#[test]
fn static_without_address_is_a_validation_error() {
    let root = TempDir::new();
    let mut tui = Tui::new(root.path(), Options::default());
    tui.handle(Event::Enter);
    tui.handle(Event::Tab);
    tui.handle(Event::Right);
    tui.handle(Event::Tab);
    let action = tui.handle(Event::CtrlSave);
    assert!(matches!(action, Action::Validation(_)));
    assert!(!network_path(root.path()).exists());
    let error = tui.selected_field().and_then(|field| field.error.as_deref());
    assert!(error.is_some());
}

#[test]
fn read_only_module_does_not_write() {
    let root = TempDir::new();
    let mut tui = Tui::new(root.path(), Options::default());
    feed(&mut tui, &text_events("about"));
    tui.handle(Event::Enter);
    assert_eq!(tui.selected_module().unwrap().title, "About");
    assert_eq!(tui.handle(Event::CtrlSave), Action::Continue);
    assert!(!network_path(root.path()).exists());
}

#[test]
fn escape_returns_to_the_list_then_cancels() {
    let root = TempDir::new();
    let mut tui = Tui::new(root.path(), Options::default());
    tui.handle(Event::Enter);
    assert_eq!(tui.focus, Focus::Form);
    tui.handle(Event::Escape);
    assert_eq!(tui.focus, Focus::List);
    assert_eq!(tui.handle(Event::Escape), Action::Cancelled);
}

#[test]
fn q_cancels_from_the_list() {
    let root = TempDir::new();
    let mut tui = Tui::new(root.path(), Options::default());
    assert_eq!(tui.handle(Event::Char('q')), Action::Cancelled);
}
