// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[derive(Clone, Debug, PartialEq)]
struct Bytes(Vec<u8>);

impl Case for Bytes {
    type Obs = usize;

    fn shrink(&self) -> Vec<Self> {
        let mut candidates = Vec::new();
        if self.0.len() > 1 {
            candidates.push(Self(self.0[1..].to_vec()));
        }
        if !self.0.is_empty() {
            candidates.push(Self(self.0[..self.0.len() - 1].to_vec()));
        }
        candidates
    }

    fn to_toml(&self) -> String {
        format!("len = {}", self.0.len())
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("ret = {obs}")
    }
}

#[test]
fn greedy_reduces_while_the_predicate_holds() {
    let case = Bytes(vec![7; 20]);
    let shrunk = greedy(&case, |candidate| candidate.0.len() >= 2, 200);
    assert_eq!(shrunk.0.len(), 2);
}

#[test]
fn greedy_stops_at_the_step_bound() {
    let case = Bytes(vec![7; 20]);
    let shrunk = greedy(&case, |candidate| !candidate.0.is_empty(), 3);
    assert_eq!(shrunk.0.len(), 17);
}

#[test]
fn greedy_leaves_a_case_when_no_candidate_mismatches() {
    let case = Bytes(vec![7]);
    let shrunk = greedy(&case, |_| false, 200);
    assert_eq!(shrunk, case);
}
