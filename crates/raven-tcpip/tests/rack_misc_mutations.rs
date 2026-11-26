// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K363: durable mutation evidence for the K334 RACK functions.
//!
//! The mutation matrix is committed in `docs/rack-k334-mutations.md`. This file adds the two
//! named tests K350 found missing (`rack_fini`, `rack_dtor`) and checks that the durable matrix
//! still covers the exact 34 K350 functions with a named test for every row.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use raven_tcpip::{RackMisc, TCPS_ESTABLISHED};

/// The 34 K334 donor functions that K350 lists as mapping/test but no durable mutation.
const K350_FUNCTIONS: [&str; 34] = [
    "rack_alloc",
    "rack_alloc_full_limit",
    "rack_alloc_limit",
    "rack_free_trim",
    "rack_free",
    "rack_get_decrease",
    "rack_decrease_highrtt",
    "rack_set_prtt_target",
    "rack_check_probe_rtt",
    "rack_grab_rtt",
    "rack_clone_rsm",
    "rack_merge_rsm",
    "rack_set_pace_segments",
    "rack_init_fsb_block",
    "rack_init_fsb",
    "rack_deferred_init",
    "rack_chg_query",
    "rack_switch_failed",
    "rack_init_outstanding",
    "rack_init",
    "rack_sndbuf_autoscale",
    "rack_mtu_change",
    "rack_set_dgp",
    "rack_set_profile",
    "rack_add_deferred_option",
    "rack_stack_information",
    "rack_inherit",
    "rack_apply_deferred_options",
    "rack_pru_options",
    "rack_set_sockopt",
    "rack_fill_info",
    "rack_get_sockopt",
    "rack_ctloutput",
    "rack_ctor",
];

#[derive(Clone, Debug)]
struct MutationRow {
    function: String,
    test: Vec<String>,
    mutation: String,
    revert: String,
    status: String,
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}

fn matrix_rows() -> Vec<MutationRow> {
    let path = repo_path("docs/rack-k334-mutations.md");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 6 {
            continue;
        }
        let Some(function) = cells[0]
            .strip_prefix('`')
            .and_then(|cell| cell.strip_suffix('`'))
        else {
            continue;
        };
        if function == "Function" || cells.iter().all(|cell| is_separator(cell)) {
            continue;
        }
        let tests: Vec<String> = cells[2]
            .split(',')
            .map(|name| name.trim().trim_matches('`').trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        rows.push(MutationRow {
            function: function.trim().to_owned(),
            test: tests,
            mutation: cells[3].to_owned(),
            revert: cells[4].to_owned(),
            status: cells[5].to_owned(),
        });
    }
    rows
}

fn is_separator(cell: &str) -> bool {
    !cell.is_empty()
        && cell
            .chars()
            .all(|character| character == '-' || character == ':' || character.is_whitespace())
}

fn all_raven_tcpip_source() -> String {
    let root = repo_path("crates/raven-tcpip");
    let mut text = String::new();
    let mut stack = vec![root];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && let Ok(file) = fs::read_to_string(&path)
            {
                text.push_str(&file);
                text.push('\n');
            }
        }
    }
    text
}

#[test]
fn rack_misc_fini_releases_every_map_and_clears_the_state() {
    let mut rack = RackMisc::new();
    rack.tp.t_state = TCPS_ESTABLISHED;
    rack.rack_init().expect("rack_init");
    let first = rack.rack_alloc().expect("first map");
    let second = rack.rack_alloc().expect("second map");
    assert!(rack.fsb.mbuf.is_some());
    assert!(rack.initialized);

    let freed = rack.rack_fini();
    assert_eq!(freed, 2, "both allocated maps are freed");
    assert!(!rack.initialized, "fini clears the initialized flag");
    assert!(rack.sendmaps.is_empty(), "fini drops every sendmap");
    assert!(rack.fsb.mbuf.is_none(), "fini releases the fast-send block");
    assert_eq!(rack.free_count, 0, "fini clears the free list");
    assert_eq!(rack.maps_alloced, 0, "fini resets the allocation counter");
    assert!(rack.deferred.is_empty(), "fini clears deferred options");
    assert!(!rack.fsb_inited);
    assert!(!rack.deferred_inited);
    assert_eq!(rack.rack_fini(), 0, "a second fini is a no-op");
    assert_ne!(first, second);
}

#[test]
fn rack_misc_dtor_leaves_the_buffer_untouched() {
    let mut memory = [0x55; 32];
    RackMisc::rack_dtor(&mut memory);
    assert!(
        memory.iter().all(|byte| *byte == 0x55),
        "rack_dtor must not write the released buffer"
    );

    assert_eq!(RackMisc::rack_ctor(&mut memory), 0);
    assert!(memory.iter().all(|byte| *byte == 0));
    RackMisc::rack_dtor(&mut memory);
    assert!(memory.iter().all(|byte| *byte == 0));
}

#[test]
fn rack_misc_k334_matrix_covers_every_k350_function() {
    let rows = matrix_rows();
    let mut by_name: BTreeMap<&str, &MutationRow> = BTreeMap::new();
    for row in &rows {
        assert!(
            by_name.insert(row.function.as_str(), row).is_none(),
            "{} appears twice in the durable matrix",
            row.function
        );
    }

    for function in K350_FUNCTIONS {
        let row = by_name
            .get(function)
            .unwrap_or_else(|| panic!("K350 function {function} is not in the durable matrix"));
        assert!(!row.mutation.trim().is_empty(), "{function} mutation");
        assert!(!row.revert.trim().is_empty(), "{function} revert result");
        assert!(row.status.eq_ignore_ascii_case("PASS"), "{function} status");
        assert!(!row.test.is_empty(), "{function} test");
    }
    for function in ["rack_fini", "rack_dtor"] {
        assert!(
            by_name.contains_key(function),
            "K363 must add a named test row for {function}"
        );
    }

    let source = all_raven_tcpip_source();
    for row in &rows {
        for test in &row.test {
            assert!(
                source.contains(&format!("fn {test}(")),
                "{} names a test absent from the source: {test}",
                row.function
            );
        }
    }

    let names: BTreeSet<&str> = rows.iter().map(|row| row.function.as_str()).collect();
    let expected: BTreeSet<&str> = K350_FUNCTIONS.into_iter().collect();
    assert_eq!(
        names.intersection(&expected).count(),
        34,
        "the durable matrix must cover all 34 K350 functions"
    );
}
