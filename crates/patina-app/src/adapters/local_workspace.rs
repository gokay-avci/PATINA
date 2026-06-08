use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use patina_types::{GaGenerationState, StructureDimensionality, StructureRecord};
use serde::Deserialize;
use serde_json::Value;

use crate::application::ports::WorkspaceCatalogPort;
use crate::models::{
    ArtifactDigest, GaGenerationObservation, GaGenerationPoint, GaLiveDigest, GaOriginCount,
    GaPopulationMember, GaRunDocument, GaTopCandidate, RunSpecDigest, StructureDigest,
    StructurePreview, StructurePreviewLattice, StructurePreviewSite, StructurePreviewSpecies,
    WorkspaceCatalog, WorkspaceRunCard,
};

#[derive(Default)]
pub struct LocalWorkspaceCatalogAdapter;

impl LocalWorkspaceCatalogAdapter {
    pub fn load_run_by_name(&self, runs_root: &Path, run_name: &str) -> Result<WorkspaceRunCard> {
        let run_path = runs_root.join(run_name);
        load_run_card(run_name, &run_path, true)
    }
}

impl WorkspaceCatalogPort for LocalWorkspaceCatalogAdapter {
    fn load_active_runs(&self, runs_root: &Path) -> Result<WorkspaceCatalog> {
        let entries = fs::read_dir(runs_root)
            .with_context(|| format!("failed to inspect {}", runs_root.display()))?;

        let mut runs = Vec::new();

        for entry in entries {
            let entry = entry.with_context(|| {
                format!("failed to inspect an entry under {}", runs_root.display())
            })?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
            if !file_type.is_dir() {
                continue;
            }

            let run_path = entry.path();
            let run_name = entry.file_name().to_string_lossy().into_owned();
            if run_name.starts_with('.') {
                continue;
            }

            match load_run_card(&run_name, &run_path, false) {
                Ok(card) => runs.push(card),
                Err(error) => runs.push(error_run_card(&run_name, &run_path, error)),
            }
        }

        runs.sort_by(|left, right| left.run_name.cmp(&right.run_name));

        Ok(WorkspaceCatalog {
            runs_root: runs_root.to_string_lossy().into_owned(),
            discovered: runs.len(),
            runs,
        })
    }
}

fn error_run_card(run_name: &str, run_path: &Path, error: anyhow::Error) -> WorkspaceRunCard {
    WorkspaceRunCard {
        run_name: run_name.to_string(),
        path: run_path.to_string_lossy().into_owned(),
        catalog_status: "error".to_string(),
        catalog_notes: vec![error.to_string()],
        has_manifest: false,
        has_checkpoint: false,
        workflow_owner: None,
        workflow_scope: None,
        system: None,
        backend: None,
        runtime_engine: None,
        lane_mode: None,
        run_spec: RunSpecDigest::default(),
        ga_live: None,
        ga_documents: Vec::new(),
        ga_generations: Vec::new(),
        top_candidates: Vec::new(),
        ga_provenance_notes: vec![error.to_string()],
        best_structure: None,
        structure_preview: None,
        artifacts: ArtifactDigest {
            manifest_path: None,
            declared_artifacts: 0,
            latest_checkpoint: None,
            generation_state_latest: None,
            structure_exports: None,
            controller_trace: None,
            search_summary: None,
        },
        manifest_payload: None,
        best_structure_record: None,
        best_evaluation_record: None,
    }
}

fn load_run_card(
    run_name: &str,
    run_path: &Path,
    include_detail: bool,
) -> Result<WorkspaceRunCard> {
    let manifest_path = run_path.join("manifest.json");
    let checkpoint_path = run_path.join("raw").join("rust_ga_checkpoint_latest.json");
    let generation_state_path = run_path.join("raw").join("ga_generation_state_latest.json");

    let has_manifest = manifest_path.exists();
    let has_checkpoint = checkpoint_path.exists();
    let has_generation_state = generation_state_path.exists();

    let mut card = WorkspaceRunCard {
        run_name: run_name.to_string(),
        path: run_path.to_string_lossy().into_owned(),
        catalog_status: if has_manifest {
            "ready".to_string()
        } else {
            "discovered".to_string()
        },
        catalog_notes: Vec::new(),
        has_manifest,
        has_checkpoint,
        workflow_owner: None,
        workflow_scope: None,
        system: None,
        backend: None,
        runtime_engine: None,
        lane_mode: None,
        run_spec: RunSpecDigest::default(),
        ga_live: None,
        ga_documents: Vec::new(),
        ga_generations: Vec::new(),
        top_candidates: Vec::new(),
        ga_provenance_notes: Vec::new(),
        best_structure: None,
        structure_preview: None,
        artifacts: ArtifactDigest {
            manifest_path: has_manifest.then(|| manifest_path.to_string_lossy().into_owned()),
            declared_artifacts: 0,
            latest_checkpoint: has_checkpoint
                .then(|| checkpoint_path.to_string_lossy().into_owned()),
            generation_state_latest: has_generation_state
                .then(|| generation_state_path.to_string_lossy().into_owned()),
            structure_exports: None,
            controller_trace: None,
            search_summary: None,
        },
        manifest_payload: None,
        best_structure_record: None,
        best_evaluation_record: None,
    };

    if include_detail {
        card.ga_documents = collect_ga_documents(run_path);
        card.ga_generations = collect_generation_observations(&run_path.join("raw"), &mut card);
        card.top_candidates = collect_top_candidates(run_path, &mut card);
    }

    if has_manifest {
        match read_manifest_bundle(&manifest_path) {
            Ok(bundle) => {
                let manifest = bundle.manifest;
                let runtime_engine = resolve_runtime_engine(&bundle.raw);
                if manifest.run_name != card.run_name {
                    card.catalog_status = "partial".to_string();
                    card.catalog_notes.push(format!(
                        "manifest run_name '{}' does not match directory '{}'",
                        manifest.run_name, card.run_name
                    ));
                }
                card.manifest_payload = Some(bundle.raw);
                card.workflow_owner = Some(manifest.workflow_owner.clone());
                card.workflow_scope = Some(manifest.workflow_scope.clone());
                card.system = Some(manifest.system.clone());
                card.backend = Some(manifest.backend.clone());
                card.runtime_engine = runtime_engine.or_else(|| Some(manifest.backend.clone()));
                card.lane_mode = Some(manifest.lane_mode.clone());
                card.run_spec = RunSpecDigest {
                    population_size: Some(manifest.population_size),
                    requested_generations: Some(manifest.requested_generations),
                    seed: manifest.search_config.seed,
                    temperature: manifest.search_config.temperature,
                    step_size: manifest.search_config.step_size,
                    parallel_contract: Some(manifest.parallel_contract),
                };
                card.artifacts.declared_artifacts = manifest.artifacts.len();
                card.artifacts.structure_exports = manifest
                    .artifacts
                    .get("structures")
                    .map(|value| run_path.join(value).to_string_lossy().into_owned());
                card.artifacts.controller_trace = manifest
                    .artifacts
                    .get("controller_trace")
                    .map(|value| run_path.join(value).to_string_lossy().into_owned());
                card.artifacts.search_summary = manifest
                    .artifacts
                    .get("search_summary")
                    .map(|value| run_path.join(value).to_string_lossy().into_owned());
            }
            Err(error) => {
                card.catalog_status = "partial".to_string();
                card.catalog_notes.push(error.to_string());
            }
        }
    }

    if has_generation_state {
        match load_ga_live_state(
            &run_path.join("raw"),
            &generation_state_path,
            include_detail,
        ) {
            Ok((ga_live, latest_state)) => {
                card.ga_live = Some(ga_live);
                if let Some(member) = best_member(&latest_state) {
                    card.best_structure = Some(StructureDigest {
                        label: member.evaluation.structure.label.clone(),
                        formula: formula_from_species(&member.evaluation.structure.species),
                        site_count: member.evaluation.structure.species.len(),
                        dimensionality: dimensionality_label(&member.evaluation.structure),
                        energy: Some(member.evaluation.energy),
                    });
                    card.structure_preview =
                        Some(structure_preview_from_record(&member.evaluation.structure));
                    if include_detail {
                        card.best_structure_record = Some(member.evaluation.structure.clone());
                        card.best_evaluation_record = Some(member.evaluation.clone());
                    }
                }
            }
            Err(error) => {
                card.catalog_status = "partial".to_string();
                card.catalog_notes.push(error.to_string());
                card.ga_provenance_notes.push(error.to_string());
            }
        }
    }

    if !has_manifest {
        card.catalog_notes
            .push("manifest.json not found for this run directory".to_string());
    }

    if card.catalog_status == "ready" && !card.catalog_notes.is_empty() {
        card.catalog_status = "partial".to_string();
    }

    Ok(card)
}

fn load_ga_live_state(
    raw_dir: &Path,
    latest_path: &Path,
    include_history: bool,
) -> Result<(GaLiveDigest, GaGenerationState)> {
    if include_history {
        let states = read_generation_history(raw_dir, latest_path)?;
        let latest_state = states
            .last()
            .cloned()
            .expect("generation history must include latest state");
        Ok((build_ga_live_digest(&states, latest_path), latest_state))
    } else {
        let latest_state = read_generation_state(latest_path)?;
        Ok((
            build_ga_live_digest(std::slice::from_ref(&latest_state), latest_path),
            latest_state,
        ))
    }
}

fn collect_ga_documents(run_path: &Path) -> Vec<GaRunDocument> {
    let raw_dir = run_path.join("raw");
    let traces_dir = run_path.join("traces");
    let outputs_dir = run_path.join("outputs");
    let inputs_dir = run_path.join("inputs");

    vec![
        ga_document(
            "Run manifest",
            run_path.join("manifest.json"),
            "workflow owner, search config, artifact map, runtime policy",
        ),
        ga_document(
            "Scott run.job",
            inputs_dir.join("run.job"),
            "stage backend, relaxation attempts, and Scott-side thresholds",
        ),
        ga_document(
            "GULP template",
            inputs_dir.join("Master.gin"),
            "external evaluator template and optimizer contract",
        ),
        ga_document(
            "Search summary",
            raw_dir.join("search_summary.json"),
            "high-level run outcome and duplicate summary",
        ),
        ga_document(
            "Latest checkpoint",
            raw_dir.join("rust_ga_checkpoint_latest.json"),
            "resumable GA controller state",
        ),
        ga_document(
            "Latest generation state",
            raw_dir.join("ga_generation_state_latest.json"),
            "current population, elites, repopulation, and topology fields",
        ),
        ga_document(
            "Generation metrics",
            traces_dir.join("generation_metrics.csv"),
            "per-generation health table used for monitoring",
        ),
        ga_document(
            "Origin metrics",
            traces_dir.join("origin_metrics.csv"),
            "seed/mutation/crossover/repopulation contribution table",
        ),
        ga_document(
            "Procedure trace",
            raw_dir.join("staged_scott_procedure_trace.json"),
            "per-request staged evaluator acceptance and failure state",
        ),
        ga_document(
            "Duplicate trace",
            raw_dir.join("staged_scott_duplicate_trace.json"),
            "topology/hashkey duplicate decisions",
        ),
        ga_document(
            "Top unique candidates",
            outputs_dir
                .join("top_unique_candidates")
                .join("top_unique_candidates.csv"),
            "ranked low-energy unique structures for inspection",
        ),
    ]
}

fn ga_document(role: &str, path: impl AsRef<Path>, detail: &str) -> GaRunDocument {
    let path = path.as_ref();
    GaRunDocument {
        role: role.to_string(),
        path: path.to_string_lossy().into_owned(),
        status: if path.exists() { "present" } else { "missing" }.to_string(),
        detail: detail.to_string(),
    }
}

fn collect_generation_observations(
    raw_dir: &Path,
    card: &mut WorkspaceRunCard,
) -> Vec<GaGenerationObservation> {
    let mut observations = Vec::new();
    let entries = match fs::read_dir(raw_dir) {
        Ok(entries) => entries,
        Err(_) => return observations,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.starts_with("generation_") || !file_name.ends_with("_summary.json") {
            continue;
        }

        match read_generation_summary(&path) {
            Ok(summary) => observations.push(summary),
            Err(error) => {
                card.catalog_status = "partial".to_string();
                card.catalog_notes.push(error.to_string());
                card.ga_provenance_notes.push(error.to_string());
            }
        }
    }

    observations.sort_by_key(|row| row.generation);
    observations
}

fn read_generation_summary(path: &Path) -> Result<GaGenerationObservation> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<GaGenerationObservationFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
        .map(Into::into)
}

fn collect_top_candidates(run_path: &Path, card: &mut WorkspaceRunCard) -> Vec<GaTopCandidate> {
    let csv_path = run_path
        .join("outputs")
        .join("top_unique_candidates")
        .join("top_unique_candidates.csv");
    if !csv_path.exists() {
        return Vec::new();
    }

    let topology_by_label = run_path
        .join("raw")
        .join("ga_generation_state_latest.json")
        .exists()
        .then(|| {
            read_generation_state(&run_path.join("raw").join("ga_generation_state_latest.json"))
                .ok()
        })
        .flatten()
        .map(|state| {
            state
                .population
                .into_iter()
                .map(|member| {
                    (
                        member.evaluation.structure.label,
                        member.topology.canonical_hashkey,
                    )
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    let raw = match fs::read_to_string(&csv_path) {
        Ok(raw) => raw,
        Err(error) => {
            card.catalog_status = "partial".to_string();
            card.catalog_notes
                .push(format!("failed to read {}: {error}", csv_path.display()));
            return Vec::new();
        }
    };

    let mut rows = Vec::new();
    for (line_index, line) in raw.lines().enumerate() {
        if line_index == 0 || line.trim().is_empty() {
            continue;
        }
        match parse_top_candidate_line(line, run_path, &topology_by_label) {
            Ok(row) => rows.push(row),
            Err(error) => {
                card.catalog_status = "partial".to_string();
                card.catalog_notes.push(format!(
                    "failed to parse {} line {}: {error}",
                    csv_path.display(),
                    line_index + 1
                ));
            }
        }
    }
    rows.sort_by_key(|row| row.unique_rank);
    rows
}

fn parse_top_candidate_line(
    line: &str,
    run_path: &Path,
    topology_by_label: &std::collections::BTreeMap<String, Option<String>>,
) -> Result<GaTopCandidate> {
    let parts = line.split(',').map(str::trim).collect::<Vec<_>>();
    anyhow::ensure!(
        parts.len() >= 7,
        "expected 7 comma-separated fields, got {}",
        parts.len()
    );
    let unique_rank = parts[0].parse::<usize>()?;
    let label = parts[6].to_string();
    Ok(GaTopCandidate {
        unique_rank,
        population_rank: parts[1].parse::<usize>().ok(),
        origin: parts[2].to_string(),
        occurrences: parts[3].parse::<usize>().unwrap_or(1),
        energy: parts[4].parse::<f64>().ok(),
        converged: parts[5].eq_ignore_ascii_case("true"),
        structure_path: find_top_candidate_structure_path(run_path, unique_rank, &label),
        canonical_hashkey: topology_by_label.get(&label).cloned().flatten(),
        label,
    })
}

fn find_top_candidate_structure_path(
    run_path: &Path,
    unique_rank: usize,
    label: &str,
) -> Option<String> {
    let dir = run_path.join("outputs").join("top_unique_candidates");
    let prefix = format!("{unique_rank:02}_");
    fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        (file_name.starts_with(&prefix) && file_name.contains(label))
            .then(|| entry.path().to_string_lossy().into_owned())
    })
}

fn read_manifest_bundle(path: &Path) -> Result<RunManifestBundle> {
    let raw_text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let raw: Value = serde_json::from_str(&raw_text)
        .with_context(|| format!("failed to parse raw {}", path.display()))?;
    let manifest: RunManifestFile = serde_json::from_str(&raw_text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(RunManifestBundle { manifest, raw })
}

fn read_generation_state(path: &Path) -> Result<GaGenerationState> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn read_generation_history(raw_dir: &Path, latest_path: &Path) -> Result<Vec<GaGenerationState>> {
    let mut states = std::collections::BTreeMap::new();

    if raw_dir.exists() {
        let entries = fs::read_dir(raw_dir)
            .with_context(|| format!("failed to inspect {}", raw_dir.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| {
                format!("failed to inspect an entry under {}", raw_dir.display())
            })?;
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if !file_name.starts_with("generation_") || !file_name.ends_with("_state.json") {
                continue;
            }

            let state = read_generation_state(&path)?;
            states.insert(state.generation, state);
        }
    }

    let latest_state = read_generation_state(latest_path)?;
    states.insert(latest_state.generation, latest_state);

    Ok(states.into_values().collect())
}

fn build_ga_live_digest(states: &[GaGenerationState], latest_path: &Path) -> GaLiveDigest {
    let latest = states.last().expect("latest generation state");
    let latest_summary = summarize_generation_state(latest);
    let mut population = latest
        .population
        .iter()
        .map(|member| GaPopulationMember {
            member_id: member.member_id,
            label: member.evaluation.structure.label.clone(),
            origin: member.origin.clone(),
            energy: Some(member.evaluation.energy),
            converged: member.evaluation.converged,
            occurrences: member.occurrences,
            canonical_hashkey: member.topology.canonical_hashkey.clone(),
        })
        .collect::<Vec<_>>();
    population.sort_by(|left, right| compare_energy(left.energy, right.energy));

    GaLiveDigest {
        current_generation: latest.generation,
        population_size: latest.population.len(),
        elite_count: latest.elites.len(),
        converged_count: latest_summary.converged_count,
        repopulation_count: latest.repopulation.len(),
        best_energy: latest_summary.best_energy,
        mean_energy: latest_summary.mean_energy,
        worst_energy: latest_summary.worst_energy,
        updated_at_unix_ms: fs::metadata(latest_path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(system_time_to_unix_ms),
        history: states.iter().map(generation_point_from_state).collect(),
        population,
    }
}

fn generation_point_from_state(state: &GaGenerationState) -> GaGenerationPoint {
    let summary = summarize_generation_state(state);

    GaGenerationPoint {
        generation: state.generation,
        population_size: state.population.len(),
        elite_count: state.elites.len(),
        converged_count: summary.converged_count,
        repopulation_count: state.repopulation.len(),
        best_energy: summary.best_energy,
        mean_energy: summary.mean_energy,
        worst_energy: summary.worst_energy,
        origin_mix: summary.origin_mix,
    }
}

fn summarize_generation_state(state: &GaGenerationState) -> GenerationSummary {
    let energies = state
        .population
        .iter()
        .map(|member| member.evaluation.energy)
        .collect::<Vec<_>>();
    let best_energy = energies.iter().copied().min_by(f64::total_cmp);
    let worst_energy = energies.iter().copied().max_by(f64::total_cmp);
    let mean_energy =
        (!energies.is_empty()).then(|| energies.iter().sum::<f64>() / energies.len() as f64);
    let converged_count = state
        .population
        .iter()
        .filter(|member| member.evaluation.converged)
        .count();

    let mut origin_counts = std::collections::BTreeMap::new();
    for member in &state.population {
        *origin_counts.entry(member.origin.clone()).or_insert(0usize) += 1;
    }

    GenerationSummary {
        best_energy,
        mean_energy,
        worst_energy,
        converged_count,
        origin_mix: origin_counts
            .into_iter()
            .map(|(origin, count)| GaOriginCount { origin, count })
            .collect(),
    }
}

fn compare_energy(left: Option<f64>, right: Option<f64>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn system_time_to_unix_ms(time: std::time::SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn best_member(state: &GaGenerationState) -> Option<&patina_types::GaMemberState> {
    state.population.iter().min_by(|left, right| {
        left.evaluation
            .energy
            .partial_cmp(&right.evaluation.energy)
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn formula_from_species(species: &[String]) -> String {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for name in species {
        if let Some((_, count)) = counts.iter_mut().find(|(existing, _)| existing == name) {
            *count += 1;
            continue;
        }
        counts.push((name.clone(), 1));
    }

    counts
        .into_iter()
        .map(|(name, count)| {
            if count == 1 {
                name
            } else {
                format!("{name}{count}")
            }
        })
        .collect()
}

fn dimensionality_label(structure: &StructureRecord) -> String {
    match structure.declared_dimensionality() {
        StructureDimensionality::ZeroD => "0D".to_string(),
        StructureDimensionality::OneD => "1D".to_string(),
        StructureDimensionality::TwoD => "2D".to_string(),
        StructureDimensionality::ThreeD => "3D".to_string(),
    }
}

fn resolve_runtime_engine(raw: &Value) -> Option<String> {
    raw.pointer("/extra/procedure_plan/stages/stages/0/engine")
        .and_then(Value::as_str)
        .or_else(|| {
            raw.pointer("/extra/procedure_plan/evaluator/settings/backend_mode")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            raw.pointer("/extra/runtime_default_backend")
                .and_then(Value::as_str)
        })
        .map(ToString::to_string)
}

fn structure_preview_from_record(structure: &StructureRecord) -> StructurePreview {
    let (lattice_matrix, raw_xyz) = match structure.lattice {
        Some(matrix) => {
            let xyz = structure
                .fractional_coords
                .iter()
                .map(|coords| fractional_to_cartesian(matrix, *coords))
                .collect::<Vec<_>>();
            (matrix, xyz)
        }
        None => synthetic_cluster_lattice(&structure.fractional_coords),
    };

    let lattice = build_preview_lattice(lattice_matrix, structure.periodic_axes);
    let inverse_lattice = invert_matrix(lattice.matrix);

    let sites = structure
        .species
        .iter()
        .zip(raw_xyz.iter())
        .enumerate()
        .map(|(index, (element, xyz))| StructurePreviewSite {
            species: vec![StructurePreviewSpecies {
                element: element.clone(),
                occu: 1.0,
                oxidation_state: 0,
            }],
            abc: wrap_fractional(cartesian_to_fractional(inverse_lattice, *xyz)),
            xyz: *xyz,
            label: format!("{element}{}", index + 1),
            properties: Default::default(),
        })
        .collect();

    StructurePreview { sites, lattice }
}

fn build_preview_lattice(
    matrix: [[f64; 3]; 3],
    periodic_axes: [bool; 3],
) -> StructurePreviewLattice {
    let a = vector_norm(matrix[0]);
    let b = vector_norm(matrix[1]);
    let c = vector_norm(matrix[2]);

    StructurePreviewLattice {
        matrix,
        a,
        b,
        c,
        alpha: angle_degrees(matrix[1], matrix[2]),
        beta: angle_degrees(matrix[0], matrix[2]),
        gamma: angle_degrees(matrix[0], matrix[1]),
        volume: triple_product(matrix[0], matrix[1], matrix[2]).abs(),
        pbc: periodic_axes,
    }
}

fn synthetic_cluster_lattice(coords: &[[f64; 3]]) -> ([[f64; 3]; 3], Vec<[f64; 3]>) {
    let mut mins = [f64::INFINITY; 3];
    let mut maxs = [f64::NEG_INFINITY; 3];

    for coord in coords {
        for axis in 0..3 {
            mins[axis] = mins[axis].min(coord[axis]);
            maxs[axis] = maxs[axis].max(coord[axis]);
        }
    }

    let mut spans = [0.0; 3];
    for axis in 0..3 {
        let span = (maxs[axis] - mins[axis]).abs();
        spans[axis] = if span < 1.0 { 8.0 } else { span + 6.0 };
    }

    let centered = coords
        .iter()
        .map(|coord| {
            let mut shifted = [0.0; 3];
            for axis in 0..3 {
                let midpoint = (mins[axis] + maxs[axis]) / 2.0;
                shifted[axis] = coord[axis] - midpoint + spans[axis] / 2.0;
            }
            shifted
        })
        .collect::<Vec<_>>();

    (
        [
            [spans[0], 0.0, 0.0],
            [0.0, spans[1], 0.0],
            [0.0, 0.0, spans[2]],
        ],
        centered,
    )
}

fn fractional_to_cartesian(lattice: [[f64; 3]; 3], coords: [f64; 3]) -> [f64; 3] {
    [
        lattice[0][0] * coords[0] + lattice[1][0] * coords[1] + lattice[2][0] * coords[2],
        lattice[0][1] * coords[0] + lattice[1][1] * coords[1] + lattice[2][1] * coords[2],
        lattice[0][2] * coords[0] + lattice[1][2] * coords[1] + lattice[2][2] * coords[2],
    ]
}

fn cartesian_to_fractional(inverse_lattice: [[f64; 3]; 3], coords: [f64; 3]) -> [f64; 3] {
    [
        inverse_lattice[0][0] * coords[0]
            + inverse_lattice[0][1] * coords[1]
            + inverse_lattice[0][2] * coords[2],
        inverse_lattice[1][0] * coords[0]
            + inverse_lattice[1][1] * coords[1]
            + inverse_lattice[1][2] * coords[2],
        inverse_lattice[2][0] * coords[0]
            + inverse_lattice[2][1] * coords[1]
            + inverse_lattice[2][2] * coords[2],
    ]
}

fn wrap_fractional(coords: [f64; 3]) -> [f64; 3] {
    coords.map(|value| {
        let wrapped = value.rem_euclid(1.0);
        if wrapped >= 1.0 {
            0.0
        } else {
            wrapped
        }
    })
}

fn invert_matrix(matrix: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let determinant = triple_product(matrix[0], matrix[1], matrix[2]);
    let safe_determinant = if determinant.abs() < 1.0e-12 {
        1.0
    } else {
        determinant
    };

    let cofactors = [
        cross_product(matrix[1], matrix[2]),
        cross_product(matrix[2], matrix[0]),
        cross_product(matrix[0], matrix[1]),
    ];

    [
        [
            cofactors[0][0] / safe_determinant,
            cofactors[0][1] / safe_determinant,
            cofactors[0][2] / safe_determinant,
        ],
        [
            cofactors[1][0] / safe_determinant,
            cofactors[1][1] / safe_determinant,
            cofactors[1][2] / safe_determinant,
        ],
        [
            cofactors[2][0] / safe_determinant,
            cofactors[2][1] / safe_determinant,
            cofactors[2][2] / safe_determinant,
        ],
    ]
}

fn cross_product(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn triple_product(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let cross = cross_product(b, c);
    a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]
}

fn vector_norm(vector: [f64; 3]) -> f64 {
    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt()
}

fn angle_degrees(left: [f64; 3], right: [f64; 3]) -> f64 {
    let denom = vector_norm(left) * vector_norm(right);
    if denom <= 1.0e-12 {
        return 90.0;
    }

    let cosine =
        ((left[0] * right[0] + left[1] * right[1] + left[2] * right[2]) / denom).clamp(-1.0, 1.0);
    cosine.acos().to_degrees()
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RunManifestFile {
    artifacts: std::collections::BTreeMap<String, String>,
    backend: String,
    lane_mode: String,
    parallel_contract: String,
    population_size: usize,
    #[serde(default)]
    provenance: Option<Value>,
    requested_generations: usize,
    run_name: String,
    search_config: SearchConfigFile,
    system: String,
    #[serde(default)]
    workflow_id: Option<String>,
    workflow_owner: String,
    workflow_scope: String,
}

#[derive(Debug)]
struct RunManifestBundle {
    manifest: RunManifestFile,
    raw: Value,
}

#[derive(Debug, Default, Deserialize)]
struct SearchConfigFile {
    seed: Option<u64>,
    step_size: Option<f64>,
    temperature: Option<f64>,
}

struct GenerationSummary {
    best_energy: Option<f64>,
    mean_energy: Option<f64>,
    worst_energy: Option<f64>,
    converged_count: usize,
    origin_mix: Vec<GaOriginCount>,
}

#[derive(Debug, Deserialize)]
struct GaGenerationObservationFile {
    generation: usize,
    phase: String,
    request_count: usize,
    success_count: usize,
    failure_count: usize,
    #[serde(default)]
    failure_kind_counts: std::collections::BTreeMap<String, usize>,
    converged_count: usize,
    population_size: usize,
    valid_population_size: usize,
    duplicate_count: usize,
    duplicate_hashkey_count: usize,
    duplicate_pmoi_count: usize,
    duplicate_energy_tol_count: usize,
    repopulated_count: usize,
    best_energy: Option<f64>,
    mean_energy: Option<f64>,
    worst_energy: Option<f64>,
}

impl From<GaGenerationObservationFile> for GaGenerationObservation {
    fn from(value: GaGenerationObservationFile) -> Self {
        Self {
            generation: value.generation,
            phase: value.phase,
            request_count: value.request_count,
            success_count: value.success_count,
            failure_count: value.failure_count,
            failure_kind_counts: value.failure_kind_counts,
            converged_count: value.converged_count,
            population_size: value.population_size,
            valid_population_size: value.valid_population_size,
            duplicate_count: value.duplicate_count,
            duplicate_hashkey_count: value.duplicate_hashkey_count,
            duplicate_pmoi_count: value.duplicate_pmoi_count,
            duplicate_energy_tol_count: value.duplicate_energy_tol_count,
            repopulated_count: value.repopulated_count,
            best_energy: value.best_energy,
            mean_energy: value.mean_energy,
            worst_energy: value.worst_energy,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_digest_preserves_first_species_order() {
        let formula = formula_from_species(&[
            "Ti".to_string(),
            "N".to_string(),
            "Ti".to_string(),
            "N".to_string(),
            "N".to_string(),
        ]);

        assert_eq!(formula, "Ti2N3");
    }

    #[test]
    fn manifest_reader_accepts_current_workspace_shape() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("workspace root");
        let manifest = workspace_root
            .join("runs")
            .join("active")
            .join("ti3n4_staged_gulp_3g10p_20260421_114907")
            .join("manifest.json");

        let parsed = read_manifest_bundle(&manifest).expect("parse sample manifest");
        assert_eq!(
            parsed.manifest.run_name,
            "ti3n4_staged_gulp_3g10p_20260421_114907"
        );
        assert_eq!(parsed.manifest.workflow_owner, "scott_staged_ga");
        assert!(parsed
            .manifest
            .artifacts
            .contains_key("latest_restart_checkpoint"));
        assert_eq!(resolve_runtime_engine(&parsed.raw).as_deref(), Some("gulp"));
    }

    #[test]
    fn generation_history_builds_live_digest_for_sample_run() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("workspace root");
        let run_root = workspace_root
            .join("runs")
            .join("active")
            .join("ti3n4_staged_gulp_3g10p_20260421_114907");
        let raw_root = run_root.join("raw");
        let latest = raw_root.join("ga_generation_state_latest.json");

        let states = read_generation_history(&raw_root, &latest).expect("read generation history");
        let digest = build_ga_live_digest(&states, &latest);

        assert_eq!(states.len(), 4);
        assert_eq!(states.first().expect("first state").generation, 0);
        assert_eq!(digest.current_generation, 3);
        assert_eq!(digest.population_size, 10);
        assert_eq!(digest.history.len(), 4);
        assert_eq!(digest.population.len(), 10);
        assert_eq!(digest.population[0].origin, "REPOPR");
    }

    #[test]
    fn workspace_catalog_serializes_to_json_value() {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("workspace root");
        let runs_root = workspace_root.join("runs").join("active");

        let catalog = LocalWorkspaceCatalogAdapter
            .load_active_runs(&runs_root)
            .expect("load active runs");

        let json = serde_json::to_value(&catalog).expect("serialize workspace catalog");
        assert!(json.get("runs").is_some());
    }

    #[test]
    fn structure_preview_wraps_periodic_structure() {
        let record = StructureRecord {
            label: "periodic".to_string(),
            species: vec!["Ti".to_string(), "N".to_string()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            lattice: Some([[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 5.0]]),
            periodic_axes: [true, true, true],
        };

        let preview = structure_preview_from_record(&record);

        assert_eq!(preview.sites.len(), 2);
        assert_eq!(preview.lattice.a, 5.0);
        assert_eq!(preview.sites[1].xyz, [2.5, 2.5, 2.5]);
        assert_eq!(preview.sites[1].abc, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn structure_preview_synthesizes_cluster_lattice() {
        let record = StructureRecord {
            label: "cluster".to_string(),
            species: vec!["Ti".to_string(), "N".to_string()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [1.5, -2.0, 3.0]],
            lattice: None,
            periodic_axes: [false, false, false],
        };

        let preview = structure_preview_from_record(&record);

        assert_eq!(preview.sites.len(), 2);
        assert_eq!(preview.lattice.pbc, [false, false, false]);
        assert!(preview.lattice.volume > 0.0);
        assert!(preview.sites.iter().all(|site| {
            site.abc
                .iter()
                .all(|value| (0.0..1.0).contains(value) || (*value - 1.0).abs() < 1.0e-12)
        }));
    }
}
