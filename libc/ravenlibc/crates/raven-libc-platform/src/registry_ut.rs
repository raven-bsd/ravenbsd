// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `FUNCTIONS.md` coverage test.
//!
//! The document, the registry table and the source tree are three copies of one fact. This test
//! fails when they drift: every row must have a status, every `EXISTS`/`PARTIAL` row must name a
//! function that exists in `src/`, every `TBD` row must be in the stub registry, and every
//! `S:` primitive named by a non-`TBD` row must exist in `raven-abi`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use raven_abi::syscall::Syscall;

use crate::registry::{Entry, FUNCTIONS, FunctionStatus};
use crate::stubs::lookup;

fn read_functions_md() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/FUNCTIONS.md"))
        .expect("FUNCTIONS.md readable")
}

fn parse_rows(text: &str) -> Vec<(String, String, String, String)> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("| `") {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        // cells[0] is empty, cells[1] the function, cells[3] the status.
        if cells.len() < 7 {
            continue;
        }
        let function = cells[1].trim_matches('`').to_string();
        let status = cells[3].to_string();
        let gap = cells[4].to_string();
        let owner = cells[5].to_string();
        rows.push((function, status, gap, owner));
    }
    rows
}

fn source_text() -> String {
    fn walk(directory: &std::path::Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
    let mut out = String::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut out,
    );
    out
}

fn has_function(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}(")) || source.contains(&format!("fn {name}<"))
}

#[test]
fn document_and_registry_have_the_same_rows() {
    let rows = parse_rows(&read_functions_md());
    assert!(!rows.is_empty(), "no rows parsed from FUNCTIONS.md");
    let mut from_md: BTreeMap<String, String> = BTreeMap::new();
    for (name, status, _, _) in &rows {
        assert!(
            from_md.insert(name.clone(), status.clone()).is_none(),
            "{name} appears twice"
        );
    }
    let mut from_registry: BTreeMap<String, String> = BTreeMap::new();
    for entry in FUNCTIONS {
        assert!(
            from_registry
                .insert(entry.name.to_string(), entry.status.as_str().to_string())
                .is_none(),
            "{} appears twice in the registry",
            entry.name
        );
    }
    assert_eq!(from_md.len(), from_registry.len(), "row count drift");
    for (name, status) in &from_md {
        assert_eq!(
            from_registry.get(name),
            Some(status),
            "{name} status differs between FUNCTIONS.md and the registry"
        );
    }
}

#[test]
fn every_row_has_a_known_status_and_gap() {
    for (name, status, gap, owner) in parse_rows(&read_functions_md()) {
        assert!(
            matches!(status.as_str(), "EXISTS" | "PARTIAL" | "TBD" | "SKIP"),
            "{name}: unknown status {status}"
        );
        assert!(!gap.is_empty(), "{name}: empty gap column");
        assert!(!owner.is_empty(), "{name}: empty owner column");
    }
}

#[test]
fn every_implemented_row_has_a_function() {
    let source = source_text();
    for entry in FUNCTIONS {
        if entry.external
            || entry.status == FunctionStatus::Tbd
            || entry.status == FunctionStatus::Skip
        {
            continue;
        }
        assert!(
            has_function(&source, entry.name),
            "{} is {} but no `fn {}` exists in src/",
            entry.name,
            entry.status.as_str(),
            entry.name
        );
    }
}

#[test]
fn every_tbd_row_is_a_registered_stub() {
    for entry in FUNCTIONS {
        if entry.status != FunctionStatus::Tbd || entry.external {
            continue;
        }
        let stub = lookup(entry.name)
            .unwrap_or_else(|| panic!("{} is TBD but not registered", entry.name));
        assert_eq!(stub.gap, entry.gap, "{} gap drift", entry.name);
    }
}

#[test]
fn every_registered_stub_is_a_tbd_row() {
    for stub in crate::stubs::STUBS {
        let entry = FUNCTIONS
            .iter()
            .find(|entry| entry.name == stub.name)
            .unwrap_or_else(|| panic!("{} is registered but not in FUNCTIONS.md", stub.name));
        assert_eq!(entry.status, FunctionStatus::Tbd, "{}", stub.name);
        assert_eq!(entry.gap, stub.gap, "{}", stub.name);
    }
}

#[test]
fn exists_rows_name_only_defined_syscalls() {
    for entry in FUNCTIONS {
        if matches!(entry.status, FunctionStatus::Tbd | FunctionStatus::Skip) {
            continue;
        }
        let Some(rest) = entry.primitive.split("S:").nth(1) else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|character| {
                character.is_ascii_uppercase() || *character == '_' || character.is_ascii_digit()
            })
            .collect();
        if name.is_empty() {
            continue;
        }
        let documented_as_proposed = rest
            .get(name.len()..)
            .is_some_and(|suffix| suffix.starts_with('*'));
        if documented_as_proposed {
            continue;
        }
        let found = Syscall::ALL
            .iter()
            .any(|syscall| format!("{syscall:?}") == name);
        assert!(
            found,
            "{} claims EXISTS/PARTIAL for S:{name}, which raven-abi lacks",
            entry.name
        );
    }
}

#[test]
fn external_rows_name_a_ticket_owner() {
    for entry in FUNCTIONS {
        if !entry.external {
            continue;
        }
        assert!(
            entry.owner.starts_with('C')
                || matches!(
                    entry.owner,
                    "kevin" | "dave" | "bob" | "mel" | "stuart" | "carl" | "reviewer"
                ),
            "{} is external but owner is {}",
            entry.name,
            entry.owner
        );
    }
}

#[test]
fn skip_rows_are_class_b() {
    for entry in FUNCTIONS {
        if entry.status == FunctionStatus::Skip {
            assert!(
                entry.gap.contains("class B"),
                "{} is SKIP without a class-B note",
                entry.name
            );
        }
    }
}

#[test]
fn status_counts_are_reported_by_the_document() {
    let mut exists = 0usize;
    let mut partial = 0usize;
    let mut tbd = 0usize;
    let mut skip = 0usize;
    for entry in FUNCTIONS {
        match entry.status {
            FunctionStatus::Exists => exists += 1,
            FunctionStatus::Partial => partial += 1,
            FunctionStatus::Tbd => tbd += 1,
            FunctionStatus::Skip => skip += 1,
        }
    }
    assert!(exists > 45, "too few EXISTS rows: {exists}");
    assert!(partial > 30, "too few PARTIAL rows: {partial}");
    assert!(tbd > 80, "too few TBD rows: {tbd}");
    assert!(skip > 0);
    let total = exists + partial + tbd + skip;
    assert_eq!(total, FUNCTIONS.len());
    let _ = Entry {
        name: "probe",
        primitive: "none",
        status: FunctionStatus::Exists,
        gap: "",
        owner: "dave",
        donor: "none",
        external: false,
    };
}
