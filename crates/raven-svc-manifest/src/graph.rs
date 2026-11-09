// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic start-order computation over enabled services.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::manifest::Manifest;
use crate::parse::MAX_SERVICES;

/// A dependency-graph error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    /// More than [`MAX_SERVICES`] services were supplied.
    TooLarge,
    /// A need names no enabled service or scheme provider.
    Missing {
        /// The service that declared the need.
        service: String,
        /// The unresolved need string.
        need: String,
    },
    /// A scheme or a service name resolves to more than one enabled service.
    AmbiguousProvider,
    /// A dependency cycle was found.
    ///
    /// The vector is the cycle in dependency order and does not repeat the starting service.
    Cycle(Vec<String>),
}

/// A start plan grouped into waves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Wave `k` holds input indices whose dependencies are all in earlier waves.
    pub waves: Vec<Vec<usize>>,
}

impl Plan {
    /// Flatten the waves into one start order.
    pub fn order(&self) -> Vec<usize> {
        self.waves.iter().flatten().copied().collect()
    }
}

/// Compute the deterministic start plan for the enabled services.
///
/// Only enabled manifests participate. A need resolves either to an enabled service name or to a
/// scheme provided by exactly one enabled service. Waves are sorted by service name; the result is
/// independent of input order.
pub fn start_order(services: &[Manifest]) -> Result<Plan, GraphError> {
    if services.len() > MAX_SERVICES {
        return Err(GraphError::TooLarge);
    }

    let enabled: Vec<usize> = services
        .iter()
        .enumerate()
        .filter(|(_, service)| service.enable)
        .map(|(index, _)| index)
        .collect();

    let mut by_name: BTreeMap<&str, usize> = BTreeMap::new();
    for &index in &enabled {
        let name = services[index].name.as_str();
        if by_name.insert(name, index).is_some() {
            return Err(GraphError::AmbiguousProvider);
        }
    }

    let mut providers: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for &index in &enabled {
        for provide in &services[index].provides {
            if !provide.starts_with("scheme:") {
                continue;
            }
            let entry = providers.entry(provide.as_str()).or_default();
            if !entry.contains(&index) {
                entry.push(index);
            }
        }
    }

    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); services.len()];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); services.len()];
    let mut indegree: Vec<usize> = vec![0; services.len()];

    for &index in &enabled {
        for need in &services[index].needs {
            let dependency = if need.starts_with("scheme:") {
                match providers.get(need.as_str()) {
                    Some(providers) if providers.len() == 1 => providers[0],
                    Some(_) => return Err(GraphError::AmbiguousProvider),
                    None => {
                        return Err(GraphError::Missing {
                            service: services[index].name.clone(),
                            need: need.clone(),
                        });
                    }
                }
            } else {
                match by_name.get(need.as_str()) {
                    Some(&dependency) => dependency,
                    None => {
                        return Err(GraphError::Missing {
                            service: services[index].name.clone(),
                            need: need.clone(),
                        });
                    }
                }
            };
            if !adjacency[index].contains(&dependency) {
                adjacency[index].push(dependency);
                dependents[dependency].push(index);
                indegree[index] += 1;
            }
        }
    }

    for &index in &enabled {
        adjacency[index].sort_by(|&left, &right| {
            services[left]
                .name
                .cmp(&services[right].name)
                .then(left.cmp(&right))
        });
    }

    let mut current: Vec<usize> = enabled
        .iter()
        .copied()
        .filter(|&index| indegree[index] == 0)
        .collect();
    sort_by_name(&mut current, services);

    let mut waves = Vec::new();
    let mut processed = 0usize;
    while !current.is_empty() {
        processed += current.len();
        waves.push(current.clone());
        let mut next = Vec::new();
        for &index in &current {
            for &dependent in &dependents[index] {
                if indegree[dependent] > 0 {
                    indegree[dependent] -= 1;
                    if indegree[dependent] == 0 {
                        next.push(dependent);
                    }
                }
            }
        }
        sort_by_name(&mut next, services);
        current = next;
    }

    if processed != enabled.len() {
        return Err(find_cycle(services, &enabled, &adjacency, &indegree));
    }
    Ok(Plan { waves })
}

fn sort_by_name(indices: &mut [usize], services: &[Manifest]) {
    indices.sort_by(|&left, &right| {
        services[left]
            .name
            .cmp(&services[right].name)
            .then(left.cmp(&right))
    });
}

/// Find one cycle left by Kahn's algorithm. The cycle omits the repeated start node.
fn find_cycle(
    services: &[Manifest],
    enabled: &[usize],
    adjacency: &[Vec<usize>],
    indegree: &[usize],
) -> GraphError {
    let mut remaining = vec![false; services.len()];
    let mut starts: Vec<usize> = enabled
        .iter()
        .copied()
        .filter(|&index| indegree[index] > 0)
        .collect();
    for &index in &starts {
        remaining[index] = true;
    }
    sort_by_name(&mut starts, services);

    let mut state = vec![0u8; services.len()];
    for start in starts {
        if state[start] != 0 {
            continue;
        }
        state[start] = 1;
        let mut path = vec![start];
        let mut stack = vec![(start, 0usize)];
        while let Some(top) = stack.last_mut() {
            let node = top.0;
            if top.1 >= adjacency[node].len() {
                stack.pop();
                state[node] = 2;
                path.pop();
                continue;
            }
            let dependency = adjacency[node][top.1];
            top.1 += 1;
            if !remaining[dependency] {
                continue;
            }
            match state[dependency] {
                0 => {
                    state[dependency] = 1;
                    path.push(dependency);
                    stack.push((dependency, 0));
                }
                1 => {
                    let position = path
                        .iter()
                        .position(|&item| item == dependency)
                        .unwrap_or(0);
                    let cycle = path[position..]
                        .iter()
                        .map(|&index| services[index].name.clone())
                        .collect();
                    return GraphError::Cycle(cycle);
                }
                _ => {}
            }
        }
    }
    GraphError::Cycle(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;
    use alloc::vec;

    fn service(name: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            exec: "/x".to_string(),
            ..Manifest::default()
        }
    }

    fn with_needs(name: &str, needs: &[&str]) -> Manifest {
        let mut manifest = service(name);
        manifest.needs = needs.iter().map(|need| need.to_string()).collect();
        manifest
    }

    fn names(plan: &Plan, services: &[Manifest]) -> Vec<Vec<String>> {
        plan.waves
            .iter()
            .map(|wave| {
                wave.iter()
                    .map(|&index| services[index].name.clone())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn linear_chain() {
        let services = vec![
            with_needs("c", &["b"]),
            with_needs("b", &["a"]),
            service("a"),
        ];
        let plan = start_order(&services).unwrap();
        assert_eq!(
            names(&plan, &services),
            vec![
                vec!["a".to_string()],
                vec!["b".to_string()],
                vec!["c".to_string()]
            ]
        );
        assert_eq!(plan.order(), vec![2, 1, 0]);
    }

    #[test]
    fn diamond_waves_are_sorted() {
        let services = vec![
            with_needs("d", &["c", "b"]),
            with_needs("c", &["a"]),
            with_needs("b", &["a"]),
            service("a"),
        ];
        let plan = start_order(&services).unwrap();
        assert_eq!(
            names(&plan, &services),
            vec![
                vec!["a".to_string()],
                vec!["b".to_string(), "c".to_string()],
                vec!["d".to_string()]
            ]
        );
    }

    #[test]
    fn scheme_provider_resolves() {
        let mut provider = service("vfsd");
        provider.provides = vec!["scheme:file".to_string()];
        let client = with_needs("shell", &["scheme:file"]);
        let services = vec![client, provider];
        let plan = start_order(&services).unwrap();
        assert_eq!(
            names(&plan, &services),
            vec![vec!["vfsd".to_string()], vec!["shell".to_string()]]
        );
    }

    #[test]
    fn missing_service_is_reported() {
        let services = vec![with_needs("client", &["missing"])];
        let error = start_order(&services).unwrap_err();
        assert_eq!(
            error,
            GraphError::Missing {
                service: "client".to_string(),
                need: "missing".to_string()
            }
        );
    }

    #[test]
    fn missing_scheme_is_reported() {
        let services = vec![with_needs("client", &["scheme:file"])];
        let error = start_order(&services).unwrap_err();
        assert_eq!(
            error,
            GraphError::Missing {
                service: "client".to_string(),
                need: "scheme:file".to_string()
            }
        );
    }

    #[test]
    fn ambiguous_provider_is_reported() {
        let mut first = service("first");
        first.provides = vec!["scheme:file".to_string()];
        let mut second = service("second");
        second.provides = vec!["scheme:file".to_string()];
        let services = vec![with_needs("client", &["scheme:file"]), first, second];
        assert_eq!(
            start_order(&services).unwrap_err(),
            GraphError::AmbiguousProvider
        );
    }

    #[test]
    fn self_dependency_is_a_cycle() {
        let services = vec![with_needs("a", &["a"])];
        assert_eq!(
            start_order(&services).unwrap_err(),
            GraphError::Cycle(vec!["a".to_string()])
        );
    }

    #[test]
    fn three_cycle_path() {
        let services = vec![
            with_needs("b", &["c"]),
            with_needs("a", &["b"]),
            with_needs("c", &["a"]),
        ];
        assert_eq!(
            start_order(&services).unwrap_err(),
            GraphError::Cycle(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn disabled_services_are_ignored() {
        let mut disabled = with_needs("disabled", &["also-missing"]);
        disabled.enable = false;
        let services = vec![service("a"), disabled];
        let plan = start_order(&services).unwrap();
        assert_eq!(names(&plan, &services), vec![vec!["a".to_string()]]);
    }

    #[test]
    fn disabled_provider_does_not_resolve_scheme() {
        let mut provider = service("provider");
        provider.provides = vec!["scheme:file".to_string()];
        provider.enable = false;
        let services = vec![with_needs("client", &["scheme:file"]), provider];
        assert!(matches!(
            start_order(&services).unwrap_err(),
            GraphError::Missing { service, need }
                if service == "client" && need == "scheme:file"
        ));
    }

    #[test]
    fn duplicate_service_names_are_ambiguous() {
        let services = vec![service("a"), service("a")];
        assert_eq!(
            start_order(&services).unwrap_err(),
            GraphError::AmbiguousProvider
        );
    }

    #[test]
    fn too_many_services() {
        let services: Vec<Manifest> = (0..=MAX_SERVICES)
            .map(|index| service(&format!("s{index}")))
            .collect();
        assert_eq!(start_order(&services).unwrap_err(), GraphError::TooLarge);
    }

    #[test]
    fn empty_input_is_an_empty_plan() {
        let plan = start_order(&[]).unwrap();
        assert!(plan.waves.is_empty());
        assert!(plan.order().is_empty());
    }

    #[test]
    fn duplicate_needs_are_one_edge() {
        let services = vec![with_needs("b", &["a", "a"]), service("a")];
        let plan = start_order(&services).unwrap();
        assert_eq!(plan.order().len(), 2);
    }

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut value = self.0;
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            self.0 = value;
            value
        }

        fn below(&mut self, limit: usize) -> usize {
            (self.next() % limit as u64) as usize
        }
    }

    fn shuffle<T>(values: &mut [T], rng: &mut Rng) {
        for index in (1..values.len()).rev() {
            let other = rng.below(index + 1);
            values.swap(index, other);
        }
    }

    #[test]
    fn plan_is_deterministic_under_input_shuffle() {
        let baseline_services = vec![
            service("a"),
            with_needs("b", &["a"]),
            with_needs("c", &["a"]),
            with_needs("d", &["b", "c"]),
            with_needs("e", &["d"]),
        ];
        let baseline = names(
            &start_order(&baseline_services).unwrap(),
            &baseline_services,
        );
        let mut rng = Rng(0x1234_5678_9abc_def0);
        let mut services = baseline_services.clone();
        for _ in 0..100 {
            shuffle(&mut services, &mut rng);
            let plan = start_order(&services).unwrap();
            assert_eq!(names(&plan, &services), baseline);
        }
    }

    #[test]
    fn random_dags_respect_every_dependency() {
        let mut rng = Rng(0x0bad_c0de_dead_beef);
        for _ in 0..100 {
            let count = 20;
            let mut services: Vec<Manifest> = (0..count)
                .map(|index| service(&format!("s{index:02}")))
                .collect();
            for (index, service) in services.iter_mut().enumerate() {
                let mut needs = Vec::new();
                for earlier in 0..index {
                    if rng.below(4) == 0 {
                        needs.push(format!("s{earlier:02}"));
                    }
                }
                service.needs = needs;
            }
            let mut shuffled = services.clone();
            shuffle(&mut shuffled, &mut rng);
            let plan = start_order(&shuffled).unwrap();
            let order = plan.order();
            let mut position = vec![0usize; count];
            for (slot, &index) in order.iter().enumerate() {
                position[index] = slot;
            }
            for (index, service) in shuffled.iter().enumerate() {
                for need in &service.needs {
                    let dependency = shuffled
                        .iter()
                        .position(|candidate| &candidate.name == need)
                        .unwrap();
                    assert!(position[dependency] < position[index]);
                }
            }
        }
    }

    #[test]
    fn cycle_with_tail_returns_only_the_cycle() {
        let services = vec![
            with_needs("tail", &["a"]),
            with_needs("a", &["b"]),
            with_needs("b", &["a"]),
        ];
        assert_eq!(
            start_order(&services).unwrap_err(),
            GraphError::Cycle(vec!["a".to_string(), "b".to_string()])
        );
    }
}
