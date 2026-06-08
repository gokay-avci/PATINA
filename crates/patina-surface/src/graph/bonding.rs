// Source-to-target mapping for this module:
// - `to_integrate_project/crystal_surface_generator/src/core/bonding.rs`

use crate::domain::{SurfaceInterfaceError, SurfaceSlab};
use crate::graph::shortest_distance_vector;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::collections::HashMap;

fn covalent_radius(element: &str) -> Option<f64> {
    Some(match element {
        "H" => 0.31,
        "B" => 0.84,
        "C" => 0.76,
        "N" => 0.71,
        "O" => 0.66,
        "F" => 0.57,
        "Si" => 1.11,
        "P" => 1.07,
        "S" => 1.05,
        "Cl" => 1.02,
        "Br" => 1.20,
        "I" => 1.39,
        "Al" => 1.21,
        "Na" => 1.66,
        "K" => 2.03,
        "Mg" => 1.41,
        "Ca" => 1.76,
        "Ti" => 1.60,
        "V" => 1.53,
        "Cr" => 1.39,
        "Mn" => 1.39,
        "Fe" => 1.32,
        "Co" => 1.26,
        "Ni" => 1.24,
        "Cu" => 1.32,
        "Zn" => 1.22,
        "Zr" => 1.75,
        "Mo" => 1.54,
        "Ag" => 1.45,
        "Cd" => 1.44,
        _ => return None,
    })
}

#[derive(Debug, Clone)]
pub(crate) struct BondEdge {
    pub(crate) u: usize,
    pub(crate) v: usize,
    pub(crate) mic: Vector3<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct BondGraph {
    pub(crate) n: usize,
    pub(crate) edges: Vec<BondEdge>,
    pub(crate) adjacency: Vec<Vec<(usize, usize)>>,
}

impl BondGraph {
    pub(crate) fn degree(&self, atom_index: usize) -> usize {
        self.adjacency
            .get(atom_index)
            .map(|list| list.len())
            .unwrap_or(0)
    }

    pub(crate) fn connected_components(&self) -> Vec<Vec<usize>> {
        let mut components = Vec::new();
        let mut seen = vec![false; self.n];
        let mut stack = Vec::new();

        for start in 0..self.n {
            if seen[start] {
                continue;
            }
            seen[start] = true;
            stack.clear();
            stack.push(start);

            let mut component = Vec::new();
            while let Some(i) = stack.pop() {
                component.push(i);
                for &(j, _) in &self.adjacency[i] {
                    if !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }

            component.sort_unstable();
            components.push(component);
        }

        components
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BondingPolicy {
    Auto,
    Zeolite,
    Ionic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BondingConfig {
    pub(crate) policy: BondingPolicy,
    pub(crate) tolerance: f64,
    pub(crate) max_cutoff: Option<f64>,
    pub(crate) target_bin_cart: Option<f64>,
    pub(crate) prune_by_coordination: bool,
}

impl Default for BondingConfig {
    fn default() -> Self {
        Self {
            policy: BondingPolicy::Auto,
            tolerance: 0.30,
            max_cutoff: None,
            target_bin_cart: None,
            prune_by_coordination: true,
        }
    }
}

pub(crate) fn build_bond_graph(
    slab: &SurfaceSlab,
    config: &BondingConfig,
) -> Result<BondGraph, SurfaceInterfaceError> {
    let atom_count = slab.atoms.len();
    if atom_count == 0 {
        return Ok(BondGraph {
            n: 0,
            edges: Vec::new(),
            adjacency: Vec::new(),
        });
    }

    let max_cutoff = config
        .max_cutoff
        .unwrap_or_else(|| default_global_max_cutoff(config));
    if !(max_cutoff.is_finite() && max_cutoff > 0.0) {
        return Err(SurfaceInterfaceError::GraphKernel(format!(
            "invalid max_cutoff computed: {max_cutoff}"
        )));
    }

    let mut candidates = Vec::<BondEdge>::with_capacity(atom_count * 4);
    for i in 0..atom_count {
        for j in (i + 1)..atom_count {
            let dv =
                shortest_distance_vector(slab, slab.atoms[i].fractional, slab.atoms[j].fractional);
            let dist2 = dv.norm_squared();
            if dist2 > max_cutoff * max_cutoff {
                continue;
            }
            let cutoff = pair_cutoff(
                &slab.atoms[i].species,
                &slab.atoms[j].species,
                config.tolerance,
            );
            if dist2 <= cutoff * cutoff {
                candidates.push(BondEdge {
                    u: i,
                    v: j,
                    mic: dv,
                });
            }
        }
    }

    match config.policy {
        BondingPolicy::Auto => {}
        BondingPolicy::Ionic => {}
        BondingPolicy::Zeolite => {
            apply_zeolite_oxygen_rule(slab, config, &mut candidates, max_cutoff)
        }
    }

    let mut keep = vec![true; candidates.len()];
    if config.prune_by_coordination {
        prune_edges_by_coordination(slab, &candidates, &mut keep);
    }
    finalize_graph(atom_count, candidates, keep)
}

#[cfg(test)]
pub(crate) fn build_bond_graph_from_neighbour_list(
    slab: &SurfaceSlab,
    neighbours: &crate::graph::neighbours::NeighbourList,
    config: &BondingConfig,
) -> Result<BondGraph, SurfaceInterfaceError> {
    let atom_count = slab.atoms.len();
    if atom_count == 0 {
        return Ok(BondGraph {
            n: 0,
            edges: Vec::new(),
            adjacency: Vec::new(),
        });
    }
    if neighbours.neighbours.len() != atom_count {
        return Err(SurfaceInterfaceError::GraphKernel(format!(
            "neighbour list length mismatch: {} vs {}",
            neighbours.neighbours.len(),
            atom_count
        )));
    }

    let max_cutoff = config
        .max_cutoff
        .unwrap_or_else(|| default_global_max_cutoff(config));
    let mut candidates = Vec::<BondEdge>::with_capacity(atom_count * 4);
    let mut seen = HashMap::<u64, ()>::new();
    seen.reserve(atom_count * 8);

    for u in 0..atom_count {
        for cand in &neighbours.neighbours[u] {
            let v = cand.j;
            if v == u {
                continue;
            }
            let (a, b) = if u < v { (u, v) } else { (v, u) };
            let key = ((a as u64) << 32) | (b as u64);
            if seen.contains_key(&key) {
                continue;
            }
            seen.insert(key, ());

            let dv =
                shortest_distance_vector(slab, slab.atoms[a].fractional, slab.atoms[b].fractional);
            let dist2 = dv.norm_squared();
            if dist2 > max_cutoff * max_cutoff {
                continue;
            }

            let cutoff = pair_cutoff(
                &slab.atoms[a].species,
                &slab.atoms[b].species,
                config.tolerance,
            );
            if dist2 <= cutoff * cutoff {
                candidates.push(BondEdge {
                    u: a,
                    v: b,
                    mic: dv,
                });
            }
        }
    }

    match config.policy {
        BondingPolicy::Auto => {}
        BondingPolicy::Ionic => {}
        BondingPolicy::Zeolite => {
            apply_zeolite_oxygen_rule(slab, config, &mut candidates, max_cutoff)
        }
    }

    let mut keep = vec![true; candidates.len()];
    if config.prune_by_coordination {
        prune_edges_by_coordination(slab, &candidates, &mut keep);
    }
    finalize_graph(atom_count, candidates, keep)
}

fn finalize_graph(
    atom_count: usize,
    candidates: Vec<BondEdge>,
    keep: Vec<bool>,
) -> Result<BondGraph, SurfaceInterfaceError> {
    let mut edges = Vec::with_capacity(candidates.len());
    for (index, edge) in candidates.into_iter().enumerate() {
        if keep[index] {
            edges.push(edge);
        }
    }

    let mut adjacency = vec![Vec::<(usize, usize)>::new(); atom_count];
    for (edge_index, edge) in edges.iter().enumerate() {
        adjacency[edge.u].push((edge.v, edge_index));
        adjacency[edge.v].push((edge.u, edge_index));
    }

    Ok(BondGraph {
        n: atom_count,
        edges,
        adjacency,
    })
}

fn default_global_max_cutoff(config: &BondingConfig) -> f64 {
    (3.5 + config.tolerance).clamp(1.2, 8.0)
}

pub(crate) fn pair_cutoff(left: &str, right: &str, tolerance: f64) -> f64 {
    let left_radius = covalent_radius(left).unwrap_or(0.77);
    let right_radius = covalent_radius(right).unwrap_or(0.77);
    (left_radius + right_radius + tolerance).clamp(0.5, 8.0)
}

fn max_coordination(element: &str) -> usize {
    match element {
        "H" => 1,
        "C" => 4,
        "N" => 4,
        "O" => 2,
        "F" | "Cl" | "Br" | "I" => 1,
        "Si" => 4,
        "Al" => 6,
        "P" => 6,
        "S" => 6,
        _ => 12,
    }
}

fn prune_edges_by_coordination(slab: &SurfaceSlab, edges: &[BondEdge], keep: &mut [bool]) {
    let atom_count = slab.atoms.len();
    let mut incident = vec![Vec::<(usize, f64)>::new(); atom_count];
    for (edge_index, edge) in edges.iter().enumerate() {
        let d2 = edge.mic.norm_squared();
        incident[edge.u].push((edge_index, d2));
        incident[edge.v].push((edge_index, d2));
    }

    let mut degree = incident.iter().map(|list| list.len()).collect::<Vec<_>>();
    for i in 0..atom_count {
        let max_neighbors = max_coordination(&slab.atoms[i].species);
        if degree[i] <= max_neighbors {
            continue;
        }
        incident[i].sort_by(|left, right| right.1.total_cmp(&left.1));
        for &(edge_index, _) in &incident[i] {
            if degree[i] <= max_neighbors {
                break;
            }
            if !keep[edge_index] {
                continue;
            }
            keep[edge_index] = false;
            let edge = &edges[edge_index];
            if degree[edge.u] > 0 {
                degree[edge.u] -= 1;
            }
            if degree[edge.v] > 0 {
                degree[edge.v] -= 1;
            }
        }
    }
}

fn apply_zeolite_oxygen_rule(
    slab: &SurfaceSlab,
    config: &BondingConfig,
    edges: &mut Vec<BondEdge>,
    max_cutoff: f64,
) {
    let atom_count = slab.atoms.len();
    let mut osi_degree = vec![0usize; atom_count];
    for edge in edges.iter() {
        let a = &slab.atoms[edge.u].species;
        let b = &slab.atoms[edge.v].species;
        if (a == "O" && (b == "Si" || b == "Al")) || (b == "O" && (a == "Si" || a == "Al")) {
            osi_degree[edge.u] += 1;
            osi_degree[edge.v] += 1;
        }
    }

    for i in 0..atom_count {
        if slab.atoms[i].species != "O" || osi_degree[i] >= 2 {
            continue;
        }

        let mut candidates = Vec::<(usize, f64, Vector3<f64>)>::new();
        for j in 0..atom_count {
            if i == j {
                continue;
            }
            let species = &slab.atoms[j].species;
            if species != "Si" && species != "Al" {
                continue;
            }
            let dv =
                shortest_distance_vector(slab, slab.atoms[i].fractional, slab.atoms[j].fractional);
            let d2 = dv.norm_squared();
            if d2 <= max_cutoff * max_cutoff {
                candidates.push((j, d2, dv));
            }
        }

        candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
        for (j, _, dv) in candidates.into_iter().take(2) {
            let already = edges
                .iter()
                .any(|edge| (edge.u == i && edge.v == j) || (edge.u == j && edge.v == i));
            if already {
                continue;
            }
            let cutoff = pair_cutoff("O", &slab.atoms[j].species, config.tolerance);
            if dv.norm_squared() <= cutoff * cutoff {
                let (u, v, mic) = if i < j { (i, j, dv) } else { (j, i, -dv) };
                edges.push(BondEdge { u, v, mic });
                osi_degree[i] += 1;
                osi_degree[j] += 1;
            }
            if osi_degree[i] >= 2 {
                break;
            }
        }
    }
}
