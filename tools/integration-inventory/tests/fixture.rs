// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The fixture workspace must classify one package per kind and detect the mirror.

#![forbid(unsafe_code)]

use std::path::Path;

use raven_integration_inventory::{Kind, check_document, inventory, write_document};

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/integration/fixture")
}

#[test]
fn the_four_kinds_and_the_mirror_are_classified() {
    let inventory = inventory(&fixture_root()).expect("fixture inventory");
    let kind_of = |package: &str| {
        inventory
            .rows
            .iter()
            .find(|row| row.package == package)
            .unwrap_or_else(|| panic!("{package} is missing: {:#?}", inventory.rows))
            .kind
    };
    assert_eq!(kind_of("fixture-core"), Kind::CoreLibrary);
    assert_eq!(kind_of("fixture-tool"), Kind::Tool);
    assert_eq!(kind_of("fixture-server"), Kind::Server);
    assert_eq!(kind_of("fixture-spike"), Kind::Spike);
    assert_eq!(kind_of("fixture-mirror"), Kind::HostTestMirror);
    assert_eq!(kind_of("fixture-kernel"), Kind::Kernel);
}

#[test]
fn a_mirror_is_recognised_from_the_source_include() {
    let inventory = inventory(&fixture_root()).expect("fixture inventory");
    let mirror = inventory
        .rows
        .iter()
        .find(|row| row.package == "fixture-mirror")
        .expect("mirror row");
    assert!(
        mirror.consumers.contains("source included by: kernel"),
        "{}",
        mirror.consumers
    );
    assert_eq!(mirror.execution, "host test, QEMU guest");
}

#[test]
fn installation_propagates_through_dependencies() {
    let inventory = inventory(&fixture_root()).expect("fixture inventory");
    let core = inventory
        .rows
        .iter()
        .find(|row| row.package == "fixture-core")
        .expect("core row");
    // fixture-server is installed by the booted profile and depends on fixture-core.
    assert_eq!(core.execution, "host test, QEMU guest");
    assert!(
        core.consumers.contains("fixture-server"),
        "{}",
        core.consumers
    );

    let tool = inventory
        .rows
        .iter()
        .find(|row| row.package == "fixture-tool")
        .expect("tool row");
    assert_eq!(tool.execution, "host test");
}

#[test]
fn an_unconsumed_library_is_reported_as_a_gap() {
    let inventory = inventory(&fixture_root()).expect("fixture inventory");
    let spike = inventory
        .rows
        .iter()
        .find(|row| row.package == "fixture-spike")
        .expect("spike row");
    assert_eq!(spike.gap, "host tests only; no target consumer");
}

#[test]
fn the_document_round_trips_through_write_and_check() {
    let root = fixture_root();
    let document = std::env::temp_dir().join(format!(
        "integration-inventory-doc-{}.md",
        std::process::id()
    ));
    std::fs::write(
        &document,
        "intro\n<!-- integration-inventory:begin -->\nstale\n\
         <!-- integration-inventory:end -->\noutro\n",
    )
    .unwrap();

    assert_eq!(
        check_document(&root, &document).unwrap(),
        Some(document.display().to_string()),
        "a stale document must be detected"
    );
    write_document(&root, &document).unwrap();
    assert_eq!(check_document(&root, &document).unwrap(), None);

    let text = std::fs::read_to_string(&document).unwrap();
    assert!(text.starts_with("intro\n<!-- integration-inventory:begin -->"));
    assert!(text.ends_with("<!-- integration-inventory:end -->\noutro\n"));
    assert!(text.contains("| component |"));
    let _ = std::fs::remove_file(&document);
}
