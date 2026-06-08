use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use patina_dreadnaut::{
    build_dreadnaut_graph_text, canonical_hashkey_via_legacy_wrapper,
    infer_atom_specs_for_candidate, parse_atoms_file,
};
use patina_search::{
    compute_structure_hashkey_radius, minimum_image_cartesian_distance_sq,
    summarize_coordination_histograms, summarize_edge_pairs, AtomSpec, HashkeyRadiusMode,
    TopologyAtom,
};
use patina_types::{Candidate, StructureRecord};
use serde_json::Value;

use crate::domain::artifacts::RunSnapshot;
use crate::domain::ga::GenerationMemberFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchOperationKind {
    TopologyMetrics,
    DreadnautGraph,
    CanonicalHashkey,
}

#[derive(Debug, Clone)]
pub struct WorkbenchOperation {
    pub kind: WorkbenchOperationKind,
    pub name: &'static str,
    pub summary: &'static str,
    pub upstream: &'static str,
    pub downstream: &'static str,
}

pub fn registry() -> Vec<WorkbenchOperation> {
    vec![
        WorkbenchOperation {
            kind: WorkbenchOperationKind::TopologyMetrics,
            name: "Topology Metrics",
            summary: "Compute fast structural summaries from the selected structure.",
            upstream: "Selected generation member -> relaxed structure -> atom specs -> cutoff radius",
            downstream: "Edge-pair summary, coordination histograms, species counts, hashkey cutoff context",
        },
        WorkbenchOperation {
            kind: WorkbenchOperationKind::DreadnautGraph,
            name: "Dreadnaut Graph Export",
            summary: "Generate native-facing dreadnaut graph text compatible with KLMC3 hashkey workflows.",
            upstream: "Selected generation member -> candidate structure -> atom specs -> dreadnaut partition order",
            downstream: "Graph payload for HKG/dreadnaut wrapper, topology inspection, export/copy workflows",
        },
        WorkbenchOperation {
            kind: WorkbenchOperationKind::CanonicalHashkey,
            name: "Canonical Hashkey",
            summary: "Run the repo dreadnaut wrapper on the generated graph and report the canonical hashkey.",
            upstream: "Selected structure -> dreadnaut graph -> wrapper path -> local dreadnaut availability",
            downstream: "Canonical hashkey text, duplicate/debug workflows, KLMC3-compatible identity probing",
        },
    ]
}

pub fn run_operation(
    operation: WorkbenchOperationKind,
    snapshot: &RunSnapshot,
    generation: usize,
    member_index: usize,
) -> Result<String> {
    let member = selected_member(snapshot, generation, member_index)?;
    match operation {
        WorkbenchOperationKind::TopologyMetrics => topology_metrics(snapshot, member),
        WorkbenchOperationKind::DreadnautGraph => dreadnaut_graph(snapshot, member),
        WorkbenchOperationKind::CanonicalHashkey => canonical_hashkey(snapshot, member),
    }
}

pub fn export_operation(
    operation: WorkbenchOperationKind,
    snapshot: &RunSnapshot,
    generation: usize,
    member_index: usize,
) -> Result<PathBuf> {
    let output = run_operation(operation, snapshot, generation, member_index)?;
    let ext = match operation {
        WorkbenchOperationKind::TopologyMetrics => "txt",
        WorkbenchOperationKind::DreadnautGraph => "dreadnaut",
        WorkbenchOperationKind::CanonicalHashkey => "txt",
    };
    let op_name = match operation {
        WorkbenchOperationKind::TopologyMetrics => "topology_metrics",
        WorkbenchOperationKind::DreadnautGraph => "dreadnaut_graph",
        WorkbenchOperationKind::CanonicalHashkey => "canonical_hashkey",
    };
    let dir = snapshot.run_dir.join("outputs").join("workbench");
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create workbench output dir `{}`", dir.display()))?;
    let file = dir.join(format!(
        "g{generation:04}_m{member_index:03}_{op_name}.{ext}"
    ));
    fs::write(&file, output)
        .with_context(|| format!("failed to write workbench export `{}`", file.display()))?;
    Ok(file)
}

fn selected_member(
    snapshot: &RunSnapshot,
    generation: usize,
    member_index: usize,
) -> Result<&GenerationMemberFile> {
    let state = snapshot
        .generation_state(generation)
        .ok_or_else(|| anyhow!("no generation state available for generation {generation}"))?;
    state
        .population
        .get(member_index)
        .ok_or_else(|| anyhow!("no member {member_index} in generation {generation}"))
}

fn topology_metrics(snapshot: &RunSnapshot, member: &GenerationMemberFile) -> Result<String> {
    let candidate = Candidate::from(&member.evaluation.structure);
    let atom_specs = load_atom_specs(snapshot, &candidate)?;
    let radius_mode = HashkeyRadiusMode::Ionic;
    let radius_const = 0.0;
    let species_counts = species_counts(&member.evaluation.structure);
    let radius =
        compute_structure_hashkey_radius(&species_counts, &atom_specs, radius_mode, radius_const)?;
    let atoms = topology_atoms(&member.evaluation.structure);
    let edges = structure_edges(&candidate, radius);
    let edge_pairs = summarize_edge_pairs(&atoms, &edges);
    let coordination = summarize_coordination_histograms(&atoms, &edges);

    let mut output = String::new();
    writeln!(&mut output, "operation: topology_metrics").ok();
    writeln!(&mut output, "label: {}", member.evaluation.structure.label).ok();
    writeln!(&mut output, "origin: {}", member.origin).ok();
    writeln!(&mut output, "converged: {}", member.evaluation.converged).ok();
    writeln!(
        &mut output,
        "energy: {}",
        member
            .evaluation
            .energy
            .map(|value| format!("{value:.6}"))
            .unwrap_or_else(|| "n/a".to_string())
    )
    .ok();
    writeln!(&mut output, "hashkey_radius_mode: ionic").ok();
    writeln!(&mut output, "hashkey_radius_const: 0.0").ok();
    writeln!(&mut output, "hashkey_radius: {radius:.6}").ok();
    writeln!(&mut output, "site_count: {}", candidate.len()).ok();
    writeln!(&mut output, "edge_count: {}", edges.len()).ok();
    writeln!(&mut output, "species_counts:").ok();
    for (species, count) in species_counts {
        writeln!(&mut output, "  {species}: {count}").ok();
    }
    writeln!(&mut output, "edge_pairs:").ok();
    for (pair, count) in edge_pairs {
        writeln!(&mut output, "  {pair}: {count}").ok();
    }
    writeln!(&mut output, "coordination_histograms:").ok();
    for (species, histogram) in coordination {
        let entries = histogram
            .into_iter()
            .map(|(coordination, count)| format!("{coordination}->{count}"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(&mut output, "  {species}: {entries}").ok();
    }
    Ok(output)
}

fn dreadnaut_graph(snapshot: &RunSnapshot, member: &GenerationMemberFile) -> Result<String> {
    let candidate = Candidate::from(&member.evaluation.structure);
    let atom_specs = load_atom_specs(snapshot, &candidate)?;
    let species_counts = species_counts(&member.evaluation.structure);
    let radius = compute_structure_hashkey_radius(
        &species_counts,
        &atom_specs,
        HashkeyRadiusMode::Ionic,
        0.0,
    )?;
    Ok(build_dreadnaut_graph_text(&candidate, radius, &atom_specs))
}

fn canonical_hashkey(snapshot: &RunSnapshot, member: &GenerationMemberFile) -> Result<String> {
    let graph = dreadnaut_graph(snapshot, member)?;
    let wrapper = infer_wrapper_path()?;
    let temp_name = format!(
        "patina_tui_graph_{}_{}.dreadnaut",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let graph_path = std::env::temp_dir().join(temp_name);
    fs::write(&graph_path, &graph)
        .with_context(|| format!("failed to write temp graph `{}`", graph_path.display()))?;
    let result = run_wrapper(&wrapper, &graph_path);
    let _ = fs::remove_file(&graph_path);
    result
}

fn species_counts(structure: &StructureRecord) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for species in &structure.species {
        *counts.entry(species.clone()).or_insert(0) += 1;
    }
    counts
}

fn topology_atoms(structure: &StructureRecord) -> Vec<TopologyAtom> {
    structure
        .species
        .iter()
        .zip(structure.fractional_coords.iter())
        .map(|(species, coords)| TopologyAtom {
            species: species.clone(),
            coords: *coords,
        })
        .collect()
}

fn structure_edges(candidate: &Candidate, radius: f64) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    let filtered = candidate
        .species
        .iter()
        .zip(candidate.fractional_coords.iter())
        .filter(|(species, _)| !species.eq_ignore_ascii_case("X"))
        .map(|(species, coords)| (species.clone(), *coords))
        .collect::<Vec<_>>();
    let radius_sq = radius * radius;
    for i in 0..filtered.len() {
        for j in (i + 1)..filtered.len() {
            let distance_sq = minimum_image_cartesian_distance_sq(
                filtered[i].1,
                filtered[j].1,
                candidate.lattice,
            );
            if distance_sq <= radius_sq + 1.0e-12 {
                edges.push((i, j));
            }
        }
    }
    edges
}

fn load_atom_specs(snapshot: &RunSnapshot, candidate: &Candidate) -> Result<Vec<AtomSpec>> {
    let atom_specs_override = infer_atoms_path(snapshot)
        .as_deref()
        .map(parse_atoms_file)
        .transpose()?;
    infer_atom_specs_for_candidate(candidate, atom_specs_override.as_deref())
}

fn infer_atoms_path(snapshot: &RunSnapshot) -> Option<PathBuf> {
    let run_atoms = snapshot.run_dir.join("raw").join("atoms.in");
    if run_atoms.exists() {
        return Some(run_atoms);
    }
    if let Some(extra) = snapshot.manifest.extra.as_ref() {
        if let Some(path) = extra
            .pointer("/procedure_plan/evaluator/atoms_in")
            .and_then(Value::as_str)
        {
            let candidate = PathBuf::from(path);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn infer_wrapper_path() -> Result<PathBuf> {
    let candidate = PathBuf::from("scripts").join("hkg_dreadnaut_wrapper.py");
    if candidate.exists() {
        Ok(candidate)
    } else {
        Err(anyhow!(
            "could not locate dreadnaut wrapper; expected scripts/hkg_dreadnaut_wrapper.py"
        ))
    }
}

fn run_wrapper(wrapper: &Path, graph_path: &Path) -> Result<String> {
    let hashkey = canonical_hashkey_via_legacy_wrapper(wrapper, graph_path)?;
    Ok(format!(
        "operation: canonical_hashkey\nwrapper: {}\ngraph: {}\nhashkey: {}\n",
        wrapper.display(),
        graph_path.display(),
        hashkey
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use patina_types::Candidate;
    use serde_json::json;
    use tempfile::tempdir;

    use crate::domain::artifacts::{RunSnapshot, WorkflowKind};
    use crate::domain::ga::ArtifactIndex;
    use crate::domain::run_manifest::RunManifest;

    use super::{build_dreadnaut_graph_text, infer_atoms_path, load_atom_specs, AtomSpec};

    fn atom_specs() -> Vec<AtomSpec> {
        vec![
            AtomSpec {
                species: "Mg".into(),
                covalent_radius: 1.1,
                ionic_radius: 0.86,
            },
            AtomSpec {
                species: "O".into(),
                covalent_radius: 0.73,
                ionic_radius: 1.26,
            },
        ]
    }

    #[test]
    fn dreadnaut_graph_text_sorts_color_partitions_by_population_count() {
        let candidate = Candidate {
            species: vec!["Mg".into(), "O".into(), "Mg".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "mgo".into(),
        };
        let graph = build_dreadnaut_graph_text(&candidate, 3.0, &atom_specs());
        let f_line = graph
            .lines()
            .find(|line| line.starts_with("f=["))
            .expect("f-line present");
        assert_eq!(f_line, "f=[1,|0,2,]");
    }

    fn snapshot_at(path: &std::path::Path) -> RunSnapshot {
        RunSnapshot {
            run_dir: path.to_path_buf(),
            loaded_from_run_dir: false,
            manifest: RunManifest::default(),
            workflow_kind: WorkflowKind::Unknown,
            generation_metrics: Vec::new(),
            controller_trace: Vec::new(),
            walker_trace: Vec::new(),
            generations: Default::default(),
            artifacts: ArtifactIndex {
                manifest_path: path.join("manifest.json"),
                raw_files: Vec::new(),
                output_files: Vec::new(),
            },
        }
    }

    #[test]
    fn infer_atoms_path_prefers_run_local_override() {
        let temp = tempdir().expect("tempdir");
        let raw_dir = temp.path().join("raw");
        fs::create_dir_all(&raw_dir).expect("raw dir");
        let run_atoms = raw_dir.join("atoms.in");
        fs::write(
            &run_atoms,
            "index,species,atomic_number,covalent_radius,ionic_radius\n1,Mg,12,1.41,0.72\n",
        )
        .expect("write run atoms");

        let snapshot = snapshot_at(temp.path());
        assert_eq!(
            infer_atoms_path(&snapshot).as_deref(),
            Some(run_atoms.as_path())
        );
    }

    #[test]
    fn infer_atoms_path_uses_manifest_override_when_present() {
        let temp = tempdir().expect("tempdir");
        let manifest_atoms = temp.path().join("override_atoms.in");
        fs::write(
            &manifest_atoms,
            "index,species,atomic_number,covalent_radius,ionic_radius\n1,O,8,0.66,1.40\n",
        )
        .expect("write manifest atoms");

        let mut snapshot = snapshot_at(temp.path());
        snapshot.manifest.extra = Some(json!({
            "procedure_plan": {
                "evaluator": {
                    "atoms_in": manifest_atoms
                }
            }
        }));

        assert_eq!(
            infer_atoms_path(&snapshot).as_deref(),
            Some(manifest_atoms.as_path())
        );
    }

    #[test]
    fn load_atom_specs_falls_back_to_builtin_species_without_override() {
        let temp = tempdir().expect("tempdir");
        let snapshot = snapshot_at(temp.path());
        let candidate = Candidate {
            species: vec!["Ga".into(), "As".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "as_ga".into(),
        };

        let atom_specs = load_atom_specs(&snapshot, &candidate).expect("built-in atom specs");
        assert_eq!(
            atom_specs
                .iter()
                .map(|spec| spec.species.as_str())
                .collect::<Vec<_>>(),
            vec!["Ga", "As"]
        );
    }
}
