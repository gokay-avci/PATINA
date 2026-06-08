use crate::genetic_algorithm::native_run::{
    load_rust_ga_generation_states, RustGaGenerationStateSnapshot, RustNativeGaRunError,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FamilyTreeError {
    #[error(transparent)]
    NativeRun(#[from] RustNativeGaRunError),
    #[error("no generation state snapshots were found under `{0}`")]
    NoGenerationStates(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyTreeConfig {
    pub run_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyTreeNode {
    pub node_id: String,
    pub generation: usize,
    pub member_id: usize,
    pub source_label: String,
    pub relaxed_label: String,
    pub origin: String,
    pub canonical_hashkey: Option<String>,
    pub parent_labels: Vec<String>,
    pub converged: bool,
    pub is_final_population: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyTreeEdge {
    pub child_node_id: String,
    pub parent_label: String,
    pub parent_node_id: Option<String>,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyTreeReport {
    pub final_generation: usize,
    pub final_population_size: usize,
    pub node_count: usize,
    pub edge_count: usize,
    pub root_node_count: usize,
    pub unresolved_parent_count: usize,
    pub nodes: Vec<FamilyTreeNode>,
    pub edges: Vec<FamilyTreeEdge>,
}

#[derive(Debug, Clone)]
struct IndexedNode {
    node: FamilyTreeNode,
}

pub fn run_family_tree_workflow(
    config: &FamilyTreeConfig,
) -> Result<FamilyTreeReport, FamilyTreeError> {
    let snapshots = load_rust_ga_generation_states(&config.run_dir)?;
    if snapshots.is_empty() {
        return Err(FamilyTreeError::NoGenerationStates(config.run_dir.clone()));
    }

    let indexed_nodes = build_indexed_nodes(&snapshots);
    let final_generation = snapshots
        .last()
        .map(|snapshot| snapshot.generation)
        .unwrap_or(0);
    let final_population_size = snapshots
        .last()
        .map(|snapshot| snapshot.state.population.len())
        .unwrap_or(0);
    let label_index = build_label_index(&indexed_nodes);

    let mut included = BTreeSet::<usize>::new();
    let mut queue = VecDeque::<usize>::new();
    let mut edges = Vec::<FamilyTreeEdge>::new();

    for (index, entry) in indexed_nodes.iter().enumerate() {
        if entry.node.is_final_population {
            included.insert(index);
            queue.push_back(index);
        }
    }

    while let Some(child_index) = queue.pop_front() {
        let child = &indexed_nodes[child_index].node;
        for parent_label in &child.parent_labels {
            let parent_index =
                resolve_parent_index(&indexed_nodes, &label_index, parent_label, child.generation);
            edges.push(FamilyTreeEdge {
                child_node_id: child.node_id.clone(),
                parent_label: parent_label.clone(),
                parent_node_id: parent_index.map(|index| indexed_nodes[index].node.node_id.clone()),
                resolved: parent_index.is_some(),
            });
            if let Some(parent_index) = parent_index {
                if included.insert(parent_index) {
                    queue.push_back(parent_index);
                }
            }
        }
    }

    let mut nodes = included
        .into_iter()
        .map(|index| indexed_nodes[index].node.clone())
        .collect::<Vec<_>>();
    nodes.sort_by_key(|node| (node.generation, node.member_id));
    edges.sort_by_key(|edge| {
        (
            edge.parent_node_id.is_none(),
            edge.child_node_id.clone(),
            edge.parent_label.clone(),
        )
    });

    let root_node_count = nodes
        .iter()
        .filter(|node| node.parent_labels.is_empty())
        .count();
    let unresolved_parent_count = edges.iter().filter(|edge| !edge.resolved).count();

    Ok(FamilyTreeReport {
        final_generation,
        final_population_size,
        node_count: nodes.len(),
        edge_count: edges.len(),
        root_node_count,
        unresolved_parent_count,
        nodes,
        edges,
    })
}

fn build_indexed_nodes(snapshots: &[RustGaGenerationStateSnapshot]) -> Vec<IndexedNode> {
    let final_generation = snapshots.last().map(|snapshot| snapshot.generation);
    snapshots
        .iter()
        .flat_map(|snapshot| {
            snapshot
                .state
                .population
                .iter()
                .map(move |member| IndexedNode {
                    node: FamilyTreeNode {
                        node_id: format!(
                            "g{:04}:m{:04}:{}",
                            snapshot.generation, member.member_id, member.source.label
                        ),
                        generation: snapshot.generation,
                        member_id: member.member_id,
                        source_label: member.source.label.clone(),
                        relaxed_label: member.evaluation.label.clone(),
                        origin: member.origin.clone(),
                        canonical_hashkey: member.topology.canonical_hashkey.clone(),
                        parent_labels: member
                            .lineage
                            .as_ref()
                            .map(|lineage| lineage.parent_labels.clone())
                            .unwrap_or_default(),
                        converged: member.evaluation.converged,
                        is_final_population: Some(snapshot.generation) == final_generation,
                    },
                })
        })
        .collect()
}

fn build_label_index(nodes: &[IndexedNode]) -> BTreeMap<String, Vec<usize>> {
    let mut index = BTreeMap::<String, Vec<usize>>::new();
    for (node_index, entry) in nodes.iter().enumerate() {
        index
            .entry(entry.node.source_label.clone())
            .or_default()
            .push(node_index);
        index
            .entry(entry.node.relaxed_label.clone())
            .or_default()
            .push(node_index);
    }
    index
}

fn resolve_parent_index(
    nodes: &[IndexedNode],
    label_index: &BTreeMap<String, Vec<usize>>,
    parent_label: &str,
    child_generation: usize,
) -> Option<usize> {
    let candidates = label_index.get(parent_label)?;
    candidates
        .iter()
        .copied()
        .filter(|index| nodes[*index].node.generation < child_generation)
        .max_by_key(|index| (nodes[*index].node.generation, nodes[*index].node.member_id))
}

#[cfg(test)]
mod tests {
    use super::{run_family_tree_workflow, FamilyTreeConfig};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reconstructs_family_tree_from_generation_snapshots() {
        let dir = tempdir().expect("tempdir");
        let run_dir = dir.path().join("run");
        fs::create_dir_all(run_dir.join("raw")).expect("raw dir");
        fs::write(
            run_dir.join("raw/generation_0000_state.json"),
            r#"{
  "generation": 0,
  "population": [
    {
      "member_id": 0,
      "origin": "SEED",
      "occurrences": 1,
      "source": { "label": "seed_a", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] },
      "evaluation": { "label": "seed_a_relaxed", "energy": -10.0, "converged": true, "structure": { "label": "seed_a_relaxed", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] }, "backend_run_dir": null, "primary_output_path": null },
      "lineage": { "origin_label": "seed_a", "generation": 0, "step": null, "parent_labels": [], "attempt": 1 },
      "topology": { "canonical_hashkey": "hk-a", "source_hashkey": "hk-a", "relaxed_hashkey": "hk-a" }
    },
    {
      "member_id": 1,
      "origin": "SEED",
      "occurrences": 1,
      "source": { "label": "seed_b", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] },
      "evaluation": { "label": "seed_b_relaxed", "energy": -9.0, "converged": true, "structure": { "label": "seed_b_relaxed", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] }, "backend_run_dir": null, "primary_output_path": null },
      "lineage": { "origin_label": "seed_b", "generation": 0, "step": null, "parent_labels": [], "attempt": 1 },
      "topology": { "canonical_hashkey": "hk-b", "source_hashkey": "hk-b", "relaxed_hashkey": "hk-b" }
    }
  ],
  "elites": [],
  "repopulation": []
}"#,
        )
        .expect("write g0");
        fs::write(
            run_dir.join("raw/generation_0001_state.json"),
            r#"{
  "generation": 1,
  "population": [
    {
      "member_id": 0,
      "origin": "CROSSO",
      "occurrences": 1,
      "source": { "label": "child_ab", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] },
      "evaluation": { "label": "child_ab_relaxed", "energy": -11.0, "converged": true, "structure": { "label": "child_ab_relaxed", "species": ["Mg","O"], "fractional_coords": [[0.0,0.0,0.0],[0.5,0.5,0.5]], "lattice": null, "periodic_axes": [false,false,false] }, "backend_run_dir": null, "primary_output_path": null },
      "lineage": { "origin_label": "child_ab", "generation": 1, "step": null, "parent_labels": ["seed_a","seed_b"], "attempt": 1 },
      "topology": { "canonical_hashkey": "hk-child", "source_hashkey": "hk-child", "relaxed_hashkey": "hk-child" }
    }
  ],
  "elites": [],
  "repopulation": []
}"#,
        )
        .expect("write g1");

        let report = run_family_tree_workflow(&FamilyTreeConfig {
            run_dir: run_dir.clone(),
        })
        .expect("family tree");

        assert_eq!(report.final_generation, 1);
        assert_eq!(report.final_population_size, 1);
        assert_eq!(report.node_count, 3);
        assert_eq!(report.edge_count, 2);
        assert_eq!(report.root_node_count, 2);
        assert_eq!(report.unresolved_parent_count, 0);
        assert!(report
            .edges
            .iter()
            .all(|edge| edge.parent_node_id.is_some() && edge.resolved));
    }
}
