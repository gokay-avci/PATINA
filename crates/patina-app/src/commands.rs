use std::fs;
use std::path::Path;

use patina_dreadnaut::{
    build_dreadnaut_graph_text, build_graph, canonical_hashkey_from_graph_text, graph_summary,
    infer_atom_specs_for_candidate, pair_cutoff_margins, resolve_dreadnaut_path,
};
use patina_perturber::{
    ClusterPerturbationEngine, ClusterStructure, DefaultClusterPerturbationEngine,
    OverlapMatrixFingerprintEngine, PerturbAxis, PerturbCentre, PerturbDistribution, PerturbMode,
    PerturbationConfig, StructureFingerprintEngine,
};
use patina_raspa::{MoyoSymmetryAnalyzer, PeriodicFramework, SymmetryAnalyzer, SymmetryTolerance};
use patina_sci_kernel::candidate_from_cif_path;
use patina_sci_kernel::codec::xyz::candidate_from_xyz_path;
use patina_surface::{
    DefaultSurfaceGenerationEngine, MillerIndex, SlabReductionConfig, SurfaceGenerationConfig,
    SurfaceGenerationEngine, SurfaceGenerationRequest, SurfaceParentStructure,
    SurfaceReconstructionMode, SurfaceSupercellConfig, SurfaceTerminationBias,
};
use patina_syva::{
    analyze_point_group, preprocess_geometry, search_symmetry_elements, summarize_operations,
    ClusterAtom, ClusterGeometry, SyvaInputGeometry, SyvaRunSettings,
};
use patina_types::Candidate;
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::adapters::{LocalWorkspaceCatalogAdapter, SurrealWorkspaceStoreAdapter};
use crate::application::WorkspaceCatalogSyncService;
use crate::models::{
    AppOverview, AutoEmulateCandidateDiagnostic, AutoEmulateExperiment, AutoEmulateLab,
    AutoEmulatePrediction, AutoEmulateScalarSummary, CatalogSyncReport, DataFlowLab,
    DataFoundation, DatabaseProfile, FlowStep, MaterialFlowArtifact, MaterialFlowMetric,
    MaterialFlowOperation, MaterialFlowRequest, MaterialFlowResponse, StructurePreview,
    StructurePreviewLattice, StructurePreviewSite, StructurePreviewSpecies, SurfaceLabDiagnostics,
    SurfaceLabRequest, SurfaceLabResponse, TableCount, TopologyGraphView, TopologyLabRequest,
    TopologyLabResponse, TopologyNodeView, TopologyPairMarginView, TopologySummaryView,
    WorkflowChain, WorkspaceCatalog, WorkspaceRunCard, WorkspaceRunRequest,
};
use crate::state::AppState;

#[tauri::command]
pub fn load_app_overview(state: State<'_, AppState>) -> AppOverview {
    state.overview()
}

#[tauri::command]
pub fn load_data_foundation(state: State<'_, AppState>) -> DataFoundation {
    state.data_foundation()
}

#[tauri::command]
pub async fn connect_data_store(state: State<'_, AppState>) -> Result<DatabaseProfile, String> {
    connect_data_store_model(state.inner()).await
}

#[tauri::command]
pub fn load_workspace_catalog(state: State<'_, AppState>) -> Result<WorkspaceCatalog, String> {
    load_workspace_catalog_model(state.inner())
}

#[tauri::command]
pub fn load_workspace_catalog_json(state: State<'_, AppState>) -> Result<String, String> {
    serde_json::to_string(&load_workspace_catalog_model(state.inner())?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_workspace_run_json(
    state: State<'_, AppState>,
    req: WorkspaceRunRequest,
) -> Result<String, String> {
    serde_json::to_string(&load_workspace_run_model(state.inner(), &req.run_name)?)
        .map_err(|error| error.to_string())
}

#[derive(serde::Deserialize)]
pub struct StructurePreviewRequest {
    pub path: String,
}

#[tauri::command]
pub fn load_structure_preview_json(req: StructurePreviewRequest) -> Result<String, String> {
    let candidate = candidate_from_structure_path(Path::new(&req.path))?;
    serde_json::to_string(&structure_preview_from_candidate(&candidate))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_workspace_catalog_to_store(
    state: State<'_, AppState>,
) -> Result<CatalogSyncReport, String> {
    sync_workspace_catalog_to_store_model(state.inner()).await
}

#[tauri::command]
pub async fn sync_workspace_catalog_to_store_json(
    state: State<'_, AppState>,
) -> Result<String, String> {
    serde_json::to_string(&sync_workspace_catalog_to_store_model(state.inner()).await?)
        .map_err(|error| error.to_string())
}

async fn sync_workspace_catalog_to_store_model(
    state: &AppState,
) -> Result<CatalogSyncReport, String> {
    let service = WorkspaceCatalogSyncService;
    let catalog_adapter = LocalWorkspaceCatalogAdapter;
    let store_adapter = SurrealWorkspaceStoreAdapter::new(state.database());

    service
        .sync_reference_workspace_catalog(
            state.reference_workspace_root(),
            &catalog_adapter,
            &store_adapter,
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn load_data_flow_lab(state: State<'_, AppState>) -> Result<DataFlowLab, String> {
    load_data_flow_lab_model(state.inner()).await
}

#[tauri::command]
pub fn load_autoemulate_lab_json(state: State<'_, AppState>) -> Result<String, String> {
    serde_json::to_string(&load_autoemulate_lab_model(state.inner())?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn load_app_snapshot_json(state: State<'_, AppState>) -> Result<String, String> {
    let payload = AppSnapshotPayload {
        overview: state.overview(),
        data_foundation: state.data_foundation(),
        data_flow_lab: load_data_flow_lab_model(state.inner()).await?,
        database: connect_data_store_model(state.inner()).await?,
        catalog: load_workspace_catalog_model(state.inner())?,
    };

    serde_json::to_string(&payload).map_err(|error| error.to_string())
}

async fn load_data_flow_lab_model(state: &AppState) -> Result<DataFlowLab, String> {
    let db = state
        .database()
        .client()
        .await
        .map_err(|error| error.to_string())?;

    let mut resolved_counts = Vec::new();
    for (table, role) in [
        ("nodes", "Machine identities and ownership roots"),
        (
            "workspaces",
            "Logical project roots decoupled from transient paths",
        ),
        (
            "structures",
            "Portable structure records that downstream tools can reuse",
        ),
        ("surfaces", "Surface definitions and slab-derived entities"),
        ("run_specs", "Workflow intent before execution"),
        ("runs", "Logical run lifecycle and status"),
        (
            "run_attempts",
            "Concrete execution attempts on local or remote targets",
        ),
        (
            "evaluations",
            "Scientific summaries, energies, and convergence state",
        ),
        (
            "artifacts",
            "Filesystem payloads anchored by semantic role and hash",
        ),
        ("events", "Immutable provenance events"),
        (
            "sync_ledger",
            "Import/export receipts and future node exchange",
        ),
    ] {
        resolved_counts.push(TableCount {
            table: table.to_string(),
            count: count_table_rows(&db, table).await?,
            role: role.to_string(),
        });
    }

    Ok(DataFlowLab {
        table_counts: resolved_counts,
        import_pipeline: vec![
            FlowStep {
                stage: "1. Discover workspace files".to_string(),
                detail:
                    "The local adapter reads run manifests, generation-state JSON, checkpoints, and declared artifact paths from runs/active."
                        .to_string(),
            },
            FlowStep {
                stage: "2. Normalize into typed records".to_string(),
                detail:
                    "PATINA structures, evaluations, run specs, and artifact digests are normalized before persistence so the DB stores portable scientific meaning instead of raw directory assumptions."
                        .to_string(),
            },
            FlowStep {
                stage: "3. Upsert SurrealDB authority tables".to_string(),
                detail:
                    "sync_workspace_catalog_to_store writes workspaces, runs, structures, evaluations, artifacts, events, and sync receipts with stable ids and revision tokens."
                        .to_string(),
            },
            FlowStep {
                stage: "4. Reuse records across lanes".to_string(),
                detail:
                    "Cluster browsing, topology inspection, and future sampling/surface launchers can now consume the same canonical structure records instead of reparsing raw files independently."
                        .to_string(),
            },
        ],
        export_pipeline: vec![
            FlowStep {
                stage: "1. Query a durable structure identity".to_string(),
                detail:
                    "The DB answers which structure, run, evaluation, and artifact set you mean via stable ids, provenance, and hashes."
                        .to_string(),
            },
            FlowStep {
                stage: "2. Materialize external files only when needed".to_string(),
                detail:
                    "Heavy payloads stay on disk; the DB points to the owning files through artifact manifests and content hashes."
                        .to_string(),
            },
            FlowStep {
                stage: "3. Hand structures to the next workflow".to_string(),
                detail:
                    "Surface generation, topology analysis, follow-on sampling, or peer-node sharing should consume DB-selected structure records and resolve filesystem artifacts only at execution time."
                        .to_string(),
            },
            FlowStep {
                stage: "4. Emit export and sync receipts".to_string(),
                detail:
                    "sync_ledger is the place to record that a structure or artifact left this node, was imported elsewhere, or was promoted into another workflow."
                        .to_string(),
            },
        ],
        workflow_chains: vec![
            WorkflowChain {
                name: "Cluster search -> topology identity".to_string(),
                steps: vec![
                    "runs/active/* -> catalog sync".to_string(),
                    "structures + evaluations in SurrealDB".to_string(),
                    "selected structure -> dreadnaut/hashkey analysis".to_string(),
                    "events + sync receipts anchor the result".to_string(),
                ],
            },
            WorkflowChain {
                name: "Cluster search -> follow-on sampling".to_string(),
                steps: vec![
                    "best structures imported into structures table".to_string(),
                    "run_specs record the follow-on intent".to_string(),
                    "new runs / run_attempts execute basin hopping, annealing, or lid exploration".to_string(),
                    "evaluations and artifacts remain linked to the parent identity".to_string(),
                ],
            },
            WorkflowChain {
                name: "Periodic parent -> surface lab -> ranked slabs".to_string(),
                steps: vec![
                    "periodic structure becomes a parent structure record".to_string(),
                    "surface definitions link back to parent structures".to_string(),
                    "slab artifacts and evaluations stay queryable through runs/evaluations/artifacts".to_string(),
                    "sync_ledger can eventually export the chosen slab package to another node".to_string(),
                ],
            },
        ],
        current_capabilities: vec![
            "The app can already import tracked workspace runs into authoritative SurrealDB tables through the sync command.".to_string(),
            "Structures are not just thumbnails: the DB stores portable structure payloads, formulas, dimensionality, lineage hooks, and links to evaluations.".to_string(),
            "Artifacts remain external files, but the DB records their semantic role, path, size, hash, and owner so downstream workflows can find them reliably.".to_string(),
        ],
        current_gaps: vec![
            "There is not yet a dedicated app command that ingests an arbitrary user-picked CIF/XYZ directly into the structures table outside the workspace sync path.".to_string(),
            "There is not yet a dedicated app export command that serializes a chosen structures-row into a portable exchange bundle with CIF/XYZ plus sync receipt.".to_string(),
            "run_attempts and execution_targets are in the schema but not yet fully surfaced by the current UI because the app has not wired real launch/monitor flows end-to-end.".to_string(),
        ],
    })
}

fn load_autoemulate_lab_model(state: &AppState) -> Result<AutoEmulateLab, String> {
    let analysis_root = state
        .reference_workspace_root()
        .join("runs")
        .join("analysis");
    let mut experiments = Vec::new();
    collect_autoemulate_experiments(&analysis_root, &mut experiments)?;
    experiments.sort_by(|left, right| {
        left.feature_family
            .cmp(&right.feature_family)
            .then(left.campaign_id.cmp(&right.campaign_id))
    });

    Ok(AutoEmulateLab {
        analysis_root: analysis_root.to_string_lossy().into_owned(),
        discovered: experiments.len(),
        experiments,
    })
}

fn collect_autoemulate_experiments(
    dir: &Path,
    experiments: &mut Vec<AutoEmulateExperiment>,
) -> Result<(), String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to inspect AutoEmulate analysis root `{}`: {error}",
                dir.display()
            ))
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("request.json").exists()
            && path.join("response.json").exists()
            && path.join("emulate_summary.json").exists()
        {
            experiments.push(load_autoemulate_experiment(&path)?);
            continue;
        }
        collect_autoemulate_experiments(&path, experiments)?;
    }

    Ok(())
}

fn load_autoemulate_experiment(path: &Path) -> Result<AutoEmulateExperiment, String> {
    let summary: AutoEmulateSummaryFile = read_json_file(&path.join("emulate_summary.json"))?;
    let request: AutoEmulateRequestFile = read_json_file(&path.join("request.json"))?;
    let response: AutoEmulateResponseFile = read_json_file(&path.join("response.json"))?;
    let report: Option<AutoEmulateReportFile> =
        read_json_file(&path.join("emulate_report.json")).ok();

    let candidate_context_by_id = request
        .candidate_rows
        .iter()
        .map(|row| {
            (
                row.candidate_id.clone(),
                (
                    row.metadata
                        .as_ref()
                        .and_then(|metadata| metadata.label.clone()),
                    row.features.clone(),
                ),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    let predictions = response
        .predictions
        .into_iter()
        .map(|prediction| AutoEmulatePrediction {
            label: candidate_context_by_id
                .get(&prediction.candidate_id)
                .cloned()
                .and_then(|(label, _features)| label),
            rank: prediction.rank,
            predicted_mean: prediction.means.first().copied().unwrap_or(f64::NAN),
            predicted_variance: prediction.variances.first().copied().unwrap_or(f64::NAN),
            acquisition_score: prediction.acquisition_score,
            uncertainty_score: prediction.uncertainty_score,
            features: candidate_context_by_id
                .get(&prediction.candidate_id)
                .map(|(_label, features)| features.clone())
                .unwrap_or_default(),
            candidate_id: prediction.candidate_id,
        })
        .collect::<Vec<_>>();

    let ga_run_name = Path::new(&summary.ga_run_dir)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string);

    Ok(AutoEmulateExperiment {
        experiment_name: path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("autoemulate")
            .to_string(),
        artifact_dir: path.to_string_lossy().into_owned(),
        campaign_id: summary.campaign_id,
        branch_id: summary.branch_id,
        ga_run_name,
        feature_family: summary.feature_family,
        feature_version: summary.feature_version,
        feature_count: summary.feature_count,
        training_observation_count: summary.training_observation_count,
        pending_candidate_count: summary.pending_candidate_count,
        selected_model_name: summary.selected_model_name,
        model_variant: summary.model_variant,
        fidelity: summary.fidelity,
        incumbent_target: summary.incumbent_target,
        best_ranked_candidate_id: summary.best_ranked_candidate_id,
        highest_uncertainty_candidate_id: summary.highest_uncertainty_candidate_id,
        top_acquisition_score: summary.top_acquisition_score,
        top_uncertainty_score: summary.top_uncertainty_score,
        checkpoint_path: response.checkpoint_path,
        training_target_summary: report
            .as_ref()
            .and_then(|report| report.training_target_summary.clone()),
        predicted_target_summary: report
            .as_ref()
            .and_then(|report| report.predicted_target_summary.clone()),
        predicted_variance_summary: report
            .as_ref()
            .and_then(|report| report.predicted_variance_summary.clone()),
        acquisition_score_summary: report
            .as_ref()
            .and_then(|report| report.acquisition_score_summary.clone()),
        uncertainty_score_summary: report
            .as_ref()
            .and_then(|report| report.uncertainty_score_summary.clone()),
        top_ranked_candidates: report
            .as_ref()
            .map(|report| report.top_ranked_candidates.clone())
            .unwrap_or_default(),
        highest_uncertainty_candidates: report
            .as_ref()
            .map(|report| report.highest_uncertainty_candidates.clone())
            .unwrap_or_default(),
        predictions,
        feature_names: request.feature_names,
    })
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|error| format!("failed to parse `{}`: {error}", path.display()))
}

async fn connect_data_store_model(state: &AppState) -> Result<DatabaseProfile, String> {
    state
        .database()
        .ensure_connected()
        .await
        .map_err(|error| error.to_string())
}

fn load_workspace_catalog_model(state: &AppState) -> Result<WorkspaceCatalog, String> {
    state
        .workspace_catalog()
        .load_reference_workspace_catalog(state.reference_workspace_root())
        .map_err(|error| error.to_string())
}

fn load_workspace_run_model(state: &AppState, run_name: &str) -> Result<WorkspaceRunCard, String> {
    let runs_root = state.reference_workspace_root().join("runs").join("active");
    LocalWorkspaceCatalogAdapter
        .load_run_by_name(&runs_root, run_name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_surface_lab(req: SurfaceLabRequest) -> Result<SurfaceLabResponse, String> {
    preview_surface_lab_model(req)
}

#[tauri::command]
pub fn preview_surface_lab_json(req: SurfaceLabRequest) -> Result<String, String> {
    serde_json::to_string(&preview_surface_lab_model(req)?).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_material_flow_json(req: MaterialFlowRequest) -> Result<String, String> {
    serde_json::to_string(&preview_material_flow_model(req)?).map_err(|error| error.to_string())
}

fn preview_material_flow_model(req: MaterialFlowRequest) -> Result<MaterialFlowResponse, String> {
    match req.flow_id.as_str() {
        "structure_symmetry" => preview_structure_symmetry_flow(req),
        "normalize_framework" => preview_normalize_framework_flow(req),
        "perturb_cluster" => preview_perturber_flow(req),
        other => Err(format!("unknown material flow `{other}`")),
    }
}

fn candidate_from_structure_path(path: &Path) -> Result<Candidate, String> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("xyz") => candidate_from_xyz_path(path).map_err(|error| error.to_string()),
        Some("cif") | Some("mcif") => candidate_from_cif_path(path).map_err(|error| error.to_string()),
        Some(extension) => Err(format!(
            "unsupported structure input extension `.{extension}`; use CIF for periodic flows or XYZ for SYVA point symmetry"
        )),
        None => Err("structure input path has no file extension".to_string()),
    }
}

fn preview_structure_symmetry_flow(
    req: MaterialFlowRequest,
) -> Result<MaterialFlowResponse, String> {
    let candidate = candidate_from_structure_path(Path::new(&req.cif_path))?;
    let input_preview = structure_preview_from_candidate(&candidate);
    let tolerance = flow_tolerance(req.tolerance);

    if req.engine == "syva" {
        let syva = run_syva_point_symmetry(&candidate, tolerance)?;
        return Ok(MaterialFlowResponse {
            flow_id: req.flow_id,
            title: "Find structure symmetry".to_string(),
            source_path: req.cif_path,
            engine: "syva point geometry".to_string(),
            status: "computed".to_string(),
            input_kind: "CIF structure".to_string(),
            output_kind: "point-group operators".to_string(),
            metrics: syva.metrics,
            operations: syva.operations,
            artifacts: Vec::new(),
            input_preview,
            output_preview: None,
        });
    }

    let framework =
        PeriodicFramework::try_from_candidate(&candidate).map_err(|error| error.to_string())?;
    let analysis = MoyoSymmetryAnalyzer
        .analyze(
            &framework,
            SymmetryTolerance {
                position_tolerance: tolerance,
                cell_tolerance: tolerance,
            },
        )
        .map_err(|error| error.to_string())?;
    let output_preview = analysis
        .standardized_framework
        .as_ref()
        .map(|framework| structure_preview_from_candidate(&framework.to_candidate()));

    Ok(MaterialFlowResponse {
        flow_id: req.flow_id,
        title: "Find structure symmetry".to_string(),
        source_path: req.cif_path,
        engine: "moyo periodic symmetry".to_string(),
        status: "computed".to_string(),
        input_kind: "CIF framework".to_string(),
        output_kind: "space-group operators + standardized cell".to_string(),
        metrics: vec![
            MaterialFlowMetric {
                label: "Space group".to_string(),
                value: analysis
                    .hm_symbol
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
                detail: analysis
                    .international_number
                    .map(|number| format!("No. {number}")),
            },
            MaterialFlowMetric {
                label: "Hall".to_string(),
                value: analysis
                    .hall_number
                    .map(|number| number.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                detail: None,
            },
            MaterialFlowMetric {
                label: "Operators".to_string(),
                value: analysis.operations.len().to_string(),
                detail: Some(format!("{} atom orbits", analysis.orbits.len())),
            },
            MaterialFlowMetric {
                label: "Wyckoff sites".to_string(),
                value: unique_count(&analysis.wyckoff_letters).to_string(),
                detail: Some(analysis.wyckoff_letters.join(", ")),
            },
        ],
        operations: analysis
            .operations
            .iter()
            .take(12)
            .enumerate()
            .map(|(index, operation)| MaterialFlowOperation {
                label: format!("op {}", index + 1),
                kind: "periodic".to_string(),
                detail: format!(
                    "R={:?}; t=[{:.3}, {:.3}, {:.3}]",
                    operation.rotation,
                    operation.translation[0],
                    operation.translation[1],
                    operation.translation[2]
                ),
            })
            .collect(),
        artifacts: vec![
            MaterialFlowArtifact {
                label: "Standardized framework".to_string(),
                path: "in-memory:moyo/std_cell".to_string(),
            },
            MaterialFlowArtifact {
                label: "Primitive standardized framework".to_string(),
                path: "in-memory:moyo/prim_std_cell".to_string(),
            },
        ],
        input_preview,
        output_preview,
    })
}

fn preview_normalize_framework_flow(
    req: MaterialFlowRequest,
) -> Result<MaterialFlowResponse, String> {
    let candidate = candidate_from_structure_path(Path::new(&req.cif_path))?;
    let input_preview = structure_preview_from_candidate(&candidate);
    let framework =
        PeriodicFramework::try_from_candidate(&candidate).map_err(|error| error.to_string())?;
    let tolerance = flow_tolerance(req.tolerance);
    let analysis = MoyoSymmetryAnalyzer
        .analyze(
            &framework,
            SymmetryTolerance {
                position_tolerance: tolerance,
                cell_tolerance: tolerance,
            },
        )
        .map_err(|error| error.to_string())?;
    let target = req
        .normalization_target
        .as_deref()
        .unwrap_or("standardized");
    let normalized = match target {
        "primitive_standardized" => analysis.primitive_standardized_framework.as_ref(),
        _ => analysis.standardized_framework.as_ref(),
    }
    .ok_or_else(|| "moyo did not return a normalized framework".to_string())?;

    Ok(MaterialFlowResponse {
        flow_id: req.flow_id,
        title: "Normalise framework".to_string(),
        source_path: req.cif_path,
        engine: "moyo normalization".to_string(),
        status: "computed".to_string(),
        input_kind: "CIF framework".to_string(),
        output_kind: target.replace('_', " "),
        metrics: vec![
            MaterialFlowMetric {
                label: "Input atoms".to_string(),
                value: framework.atom_count().to_string(),
                detail: Some(framework.label.clone()),
            },
            MaterialFlowMetric {
                label: "Output atoms".to_string(),
                value: normalized.atom_count().to_string(),
                detail: Some(normalized.label.clone()),
            },
            MaterialFlowMetric {
                label: "Space group".to_string(),
                value: analysis.hm_symbol.unwrap_or_else(|| "-".to_string()),
                detail: analysis
                    .international_number
                    .map(|number| format!("No. {number}")),
            },
            MaterialFlowMetric {
                label: "Target".to_string(),
                value: target.replace('_', " "),
                detail: Some(format!("tolerance {:.1e}", tolerance)),
            },
        ],
        operations: analysis
            .operations
            .iter()
            .take(8)
            .enumerate()
            .map(|(index, operation)| MaterialFlowOperation {
                label: format!("basis op {}", index + 1),
                kind: "normalization".to_string(),
                detail: format!("R={:?}; t={:?}", operation.rotation, operation.translation),
            })
            .collect(),
        artifacts: vec![MaterialFlowArtifact {
            label: "Normalized framework".to_string(),
            path: format!("in-memory:moyo/{target}"),
        }],
        input_preview,
        output_preview: Some(structure_preview_from_candidate(&normalized.to_candidate())),
    })
}

fn preview_perturber_flow(req: MaterialFlowRequest) -> Result<MaterialFlowResponse, String> {
    let candidate = candidate_from_structure_path(Path::new(&req.cif_path))?;
    let input_preview = structure_preview_from_candidate(&candidate);
    let source = ClusterStructure::try_from_candidate(&candidate).map_err(|error| {
        format!("patina-perturber requires a zero-dimensional cluster input, usually XYZ: {error}")
    })?;
    let count = req.perturbation_count.unwrap_or(8).clamp(1, 48);
    let mode = match req.perturbation_mode.as_deref() {
        Some("sp") | Some("SP") | Some("Sp") => PerturbMode::Sp,
        _ => PerturbMode::S,
    };
    let config = PerturbationConfig {
        sigma: req.perturbation_sigma.unwrap_or(0.05),
        max_displacement: req.perturbation_max_displacement.or(Some(0.15)),
        validate_min_distance: req.perturbation_min_distance,
        seed: req.perturbation_seed.or(Some(7)),
        dist: PerturbDistribution::Gaussian,
        mode,
        axis: PerturbAxis::Auto,
        centre: PerturbCentre::Com,
        max_attempts: 400,
        ..PerturbationConfig::default()
    };
    let batch = DefaultClusterPerturbationEngine
        .generate(&source, config, count)
        .map_err(|error| error.to_string())?;
    let first = batch
        .variants
        .first()
        .ok_or_else(|| "perturber returned no variants".to_string())?;
    let fingerprint_engine = OverlapMatrixFingerprintEngine::default();
    let base_fp = fingerprint_engine
        .fingerprint(&source)
        .map_err(|error| error.to_string())?;
    let first_fp = fingerprint_engine
        .fingerprint(first)
        .map_err(|error| error.to_string())?;
    let first_distance = euclidean_distance(&base_fp.values, &first_fp.values)?;
    let drms = cluster_distance_rms(&source, first)?;
    let min_distance = first.min_distance().unwrap_or(0.0);
    let output_candidate = Candidate::from(first);

    Ok(MaterialFlowResponse {
        flow_id: req.flow_id,
        title: "Perturb cluster".to_string(),
        source_path: req.cif_path,
        engine: "patina-perturber".to_string(),
        status: "computed".to_string(),
        input_kind: "XYZ cluster".to_string(),
        output_kind: "perturbed cluster".to_string(),
        metrics: vec![
            MaterialFlowMetric {
                label: "Variants".to_string(),
                value: count.to_string(),
                detail: Some(format!("sigma {:.4}", config.sigma)),
            },
            MaterialFlowMetric {
                label: "Fingerprint delta".to_string(),
                value: format!("{first_distance:.6}"),
                detail: Some("overlap-matrix first variant".to_string()),
            },
            MaterialFlowMetric {
                label: "dRMS".to_string(),
                value: format!("{drms:.6}"),
                detail: Some("first variant".to_string()),
            },
            MaterialFlowMetric {
                label: "Min distance".to_string(),
                value: format!("{min_distance:.4} A"),
                detail: config
                    .validate_min_distance
                    .map(|value| format!("validated >= {value:.3} A")),
            },
        ],
        operations: vec![
            MaterialFlowOperation {
                label: "structure read".to_string(),
                kind: "port".to_string(),
                detail: "CIF/XYZ loader resolves provenance before perturbation".to_string(),
            },
            MaterialFlowOperation {
                label: "perturb".to_string(),
                kind: "patina-perturber".to_string(),
                detail: format!("{mode:?} Gaussian displacements around centre of mass"),
            },
            MaterialFlowOperation {
                label: "fingerprint".to_string(),
                kind: "descriptor".to_string(),
                detail: "overlap-matrix distance records structural movement".to_string(),
            },
        ],
        artifacts: vec![MaterialFlowArtifact {
            label: "Perturbation batch".to_string(),
            path: "in-memory:patina-perturber/perturbation_batch".to_string(),
        }],
        input_preview,
        output_preview: Some(structure_preview_from_candidate(&output_candidate)),
    })
}

fn euclidean_distance(left: &[f64], right: &[f64]) -> Result<f64, String> {
    if left.len() != right.len() {
        return Err(format!(
            "fingerprint dimensionality mismatch: {} vs {}",
            left.len(),
            right.len()
        ));
    }
    Ok(left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| {
            let delta = a - b;
            delta * delta
        })
        .sum::<f64>()
        .sqrt())
}

fn cluster_distance_rms(left: &ClusterStructure, right: &ClusterStructure) -> Result<f64, String> {
    if left.atoms.len() != right.atoms.len() {
        return Err(format!(
            "atom count mismatch for dRMS: {} vs {}",
            left.atoms.len(),
            right.atoms.len()
        ));
    }
    let mut count = 0usize;
    let mut acc = 0.0_f64;
    for i in 0..left.atoms.len() {
        for j in (i + 1)..left.atoms.len() {
            let left_distance = point_distance(left.atoms[i].cartesian, left.atoms[j].cartesian);
            let right_distance = point_distance(right.atoms[i].cartesian, right.atoms[j].cartesian);
            let delta = right_distance - left_distance;
            acc += delta * delta;
            count += 1;
        }
    }
    Ok(if count == 0 {
        0.0
    } else {
        (acc / count as f64).sqrt()
    })
}

fn point_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    ((left[0] - right[0]).powi(2) + (left[1] - right[1]).powi(2) + (left[2] - right[2]).powi(2))
        .sqrt()
}

struct SyvaFlowResult {
    metrics: Vec<MaterialFlowMetric>,
    operations: Vec<MaterialFlowOperation>,
}

fn run_syva_point_symmetry(
    candidate: &Candidate,
    tolerance: f64,
) -> Result<SyvaFlowResult, String> {
    let cluster = ClusterGeometry {
        label: candidate.label.clone(),
        atoms: candidate
            .species
            .iter()
            .zip(candidate.fractional_coords.iter().copied())
            .map(|(species, fractional)| ClusterAtom {
                species: species.clone(),
                cartesian: candidate
                    .lattice
                    .map(|lattice| fractional_to_cartesian(lattice, fractional))
                    .unwrap_or(fractional),
            })
            .collect(),
    };
    let input =
        SyvaInputGeometry::from_cluster_geometry(&cluster).map_err(|error| error.to_string())?;
    let settings = SyvaRunSettings {
        tolerance,
        tolerance_upper: (tolerance * 50.0).max(tolerance),
        tolerance_lower: (tolerance * 0.5).max(f64::MIN_POSITIVE),
        ..SyvaRunSettings::default()
    };
    let geometry = preprocess_geometry(&input, &settings).map_err(|error| error.to_string())?;
    let search = search_symmetry_elements(&geometry);
    let point_group = analyze_point_group(&geometry).map_err(|error| error.to_string())?;
    let classified = point_group
        .ok_or_else(|| "SYVA did not classify a point group for this geometry".to_string())?;
    let operations = summarize_operations(&classified.label, &geometry, &search);

    Ok(SyvaFlowResult {
        metrics: vec![
            MaterialFlowMetric {
                label: "Point group".to_string(),
                value: classified.label.as_str().to_string(),
                detail: Some(format!("order {}", classified.signature.order)),
            },
            MaterialFlowMetric {
                label: "Atoms".to_string(),
                value: geometry.active_atoms.len().to_string(),
                detail: Some(format!("{} classes", geometry.atom_classes.len())),
            },
            MaterialFlowMetric {
                label: "Proper rotations".to_string(),
                value: classified.signature.proper_rotation_count.to_string(),
                detail: Some(format!(
                    "principal {}",
                    classified.signature.principal_order
                )),
            },
            MaterialFlowMetric {
                label: "Reflection planes".to_string(),
                value: classified.signature.reflection_plane_count.to_string(),
                detail: Some(format!("max deviation {:.2e}", search.max_deviation)),
            },
        ],
        operations: operations
            .into_iter()
            .take(12)
            .map(|operation| MaterialFlowOperation {
                label: operation.label,
                kind: format!("{:?}", operation.kind),
                detail: format!(
                    "{} fixed atoms; max deviation {:.2e}",
                    operation.fixed_atom_count, operation.max_deviation
                ),
            })
            .collect(),
    })
}

fn flow_tolerance(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0e-5
    }
}

fn unique_count(values: &[String]) -> usize {
    values
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

fn preview_surface_lab_model(req: SurfaceLabRequest) -> Result<SurfaceLabResponse, String> {
    let cif_path = Path::new(&req.cif_path);
    let candidate = candidate_from_cif_path(cif_path).map_err(|error| error.to_string())?;
    let parent = SurfaceParentStructure::try_from_candidate(&candidate)
        .map_err(|error| error.to_string())?;
    let parent_preview = structure_preview_from_candidate(&parent.to_candidate());

    let engine = DefaultSurfaceGenerationEngine;
    let config = SurfaceGenerationConfig {
        miller: MillerIndex::new(req.h, req.k, req.l).map_err(|error| error.to_string())?,
        thickness_angstrom: req.thickness_angstrom,
        vacuum_angstrom: req.vacuum_angstrom,
        supercell: SurfaceSupercellConfig {
            repeat_a: req.repeat_a,
            repeat_b: req.repeat_b,
        },
        slab_reduction: SlabReductionConfig {
            dedup_slab: req.dedup_slab,
            ..SlabReductionConfig::default()
        },
        reconstruction: SurfaceReconstructionMode::None,
        termination_bias: SurfaceTerminationBias::Neutral,
        ..SurfaceGenerationConfig::default()
    };
    let result = engine
        .generate_surface(&SurfaceGenerationRequest {
            parent: parent.clone(),
            config: config.clone(),
        })
        .map_err(|error| error.to_string())?;
    let slab_candidate = result.slab.to_candidate();

    Ok(SurfaceLabResponse {
        source_path: req.cif_path,
        parent_label: parent.label.clone(),
        parent_preview,
        slab_preview: structure_preview_from_candidate(&slab_candidate),
        diagnostics: SurfaceLabDiagnostics {
            slab_label: result.slab.label,
            miller: [config.miller.h, config.miller.k, config.miller.l],
            parent_atom_count: parent.atoms.len(),
            slab_atom_count: result.slab.atoms.len(),
            topology_safe_cut: result.diagnostics.topology_safe_cut,
            chosen_cut_offset_angstrom: result.diagnostics.chosen_cut_offset_angstrom,
            interplanar_spacing_angstrom: result.diagnostics.interplanar_spacing_angstrom,
            broken_bond_estimate: result.diagnostics.broken_bond_estimate,
            warnings: result.diagnostics.warnings,
        },
    })
}

#[tauri::command]
pub fn load_topology_lab(
    state: State<'_, AppState>,
    req: TopologyLabRequest,
) -> Result<TopologyLabResponse, String> {
    load_topology_lab_model(state.inner(), req)
}

#[tauri::command]
pub fn load_topology_lab_json(
    state: State<'_, AppState>,
    req: TopologyLabRequest,
) -> Result<String, String> {
    serde_json::to_string(&load_topology_lab_model(state.inner(), req)?)
        .map_err(|error| error.to_string())
}

fn load_topology_lab_model(
    state: &AppState,
    req: TopologyLabRequest,
) -> Result<TopologyLabResponse, String> {
    let (source_label, source_path, candidate) = topology_candidate_from_request(state, &req)?;
    let source_preview = structure_preview_from_candidate(&candidate);
    let atom_specs =
        infer_atom_specs_for_candidate(&candidate, None).map_err(|error| error.to_string())?;

    let radius_mode = "IR".to_string();
    let radius_const = 3.34_f64;
    let radius = patina_dreadnaut::compute_hashkey_radius(
        &candidate,
        &atom_specs,
        &radius_mode,
        radius_const,
    )
    .map_err(|error| error.to_string())?;
    let graph = build_graph(&candidate, radius, &atom_specs);
    let summary = graph_summary(&graph);
    let graph_text = build_dreadnaut_graph_text(&candidate, radius, &atom_specs);

    let canonical_hashkey = match resolve_dreadnaut_path(None)
        .and_then(|path| canonical_hashkey_from_graph_text(&path, &graph_text))
    {
        Ok(hashkey) => (Some(hashkey), None),
        Err(error) => (None, Some(error.to_string())),
    };

    let mut near_cutoff_pairs = pair_cutoff_margins(&candidate, radius);
    near_cutoff_pairs.sort_by(|left, right| {
        left.margin
            .abs()
            .partial_cmp(&right.margin.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let edge_degrees = graph
        .nodes
        .iter()
        .map(|node| {
            let degree = graph
                .edges
                .iter()
                .filter(|(left, right)| *left == node.index || *right == node.index)
                .count();
            TopologyNodeView {
                index: node.index,
                species: node.species.clone(),
                degree,
            }
        })
        .collect::<Vec<_>>();

    Ok(TopologyLabResponse {
        run_name: source_label,
        source_path,
        source_preview,
        radius,
        radius_mode,
        radius_const,
        graph: TopologyGraphView {
            nodes: edge_degrees,
            edges: graph
                .edges
                .iter()
                .map(|(left, right)| [*left, *right])
                .collect(),
            color_partitions: graph.color_partitions,
        },
        summary: TopologySummaryView {
            node_count: summary.node_count,
            edge_count: summary.edge_count,
            connected_components: summary.connected_components,
            degree_histogram_by_species: summary.degree_histogram_by_species,
        },
        graph_text,
        canonical_hashkey: canonical_hashkey.0,
        hashkey_error: canonical_hashkey.1,
        near_cutoff_pairs: near_cutoff_pairs
            .into_iter()
            .take(18)
            .map(|pair| TopologyPairMarginView {
                left: pair.left,
                right: pair.right,
                left_species: pair.left_species,
                right_species: pair.right_species,
                distance: pair.distance,
                margin: pair.margin,
                is_edge: pair.is_edge,
            })
            .collect(),
    })
}

fn topology_candidate_from_request(
    state: &AppState,
    req: &TopologyLabRequest,
) -> Result<(String, Option<String>, Candidate), String> {
    if let Some(path) = req
        .structure_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let candidate = candidate_from_structure_path(Path::new(path))?;
        return Ok((path.to_string(), Some(path.to_string()), candidate));
    }

    let run_name = req
        .run_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "topology calculation needs a run name or structure path".to_string())?;
    let catalog = state
        .workspace_catalog()
        .load_reference_workspace_catalog(state.reference_workspace_root())
        .map_err(|error| error.to_string())?;
    let run = catalog
        .runs
        .into_iter()
        .find(|run| run.run_name == run_name)
        .ok_or_else(|| format!("run `{run_name}` was not found in the workspace catalog"))?;

    if let Some(path) = run
        .top_candidates
        .iter()
        .filter_map(|candidate| candidate.structure_path.as_deref())
        .find(|path| !path.trim().is_empty())
    {
        let candidate = candidate_from_structure_path(Path::new(path))?;
        return Ok((run_name.to_string(), Some(path.to_string()), candidate));
    }

    if let Some(record) = run.best_structure_record {
        return Ok((run_name.to_string(), None, Candidate::from(&record)));
    }

    Err(format!(
        "run `{run_name}` does not expose a structure path or in-memory structure record"
    ))
}

fn structure_preview_from_candidate(candidate: &patina_types::Candidate) -> StructurePreview {
    let record = patina_types::StructureRecord::from(candidate);
    let lattice = record
        .lattice
        .unwrap_or([[8.0, 0.0, 0.0], [0.0, 8.0, 0.0], [0.0, 0.0, 8.0]]);
    let sites = record
        .species
        .iter()
        .zip(record.fractional_coords.iter())
        .enumerate()
        .map(|(index, (element, abc))| StructurePreviewSite {
            species: vec![StructurePreviewSpecies {
                element: element.clone(),
                occu: 1.0,
                oxidation_state: 0,
            }],
            abc: *abc,
            xyz: fractional_to_cartesian(lattice, *abc),
            label: format!("{element}{}", index + 1),
            properties: Default::default(),
        })
        .collect();

    StructurePreview {
        sites,
        lattice: StructurePreviewLattice {
            matrix: lattice,
            a: vector_norm(lattice[0]),
            b: vector_norm(lattice[1]),
            c: vector_norm(lattice[2]),
            alpha: angle_degrees(lattice[1], lattice[2]),
            beta: angle_degrees(lattice[0], lattice[2]),
            gamma: angle_degrees(lattice[0], lattice[1]),
            volume: triple_product(lattice[0], lattice[1], lattice[2]).abs(),
            pbc: record.periodic_axes,
        },
    }
}

async fn count_table_rows(
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    table: &str,
) -> Result<usize, String> {
    db.select::<Vec<Value>>(table)
        .await
        .map(|rows| rows.len())
        .map_err(|error| error.to_string())
}

fn fractional_to_cartesian(lattice: [[f64; 3]; 3], coords: [f64; 3]) -> [f64; 3] {
    [
        lattice[0][0] * coords[0] + lattice[1][0] * coords[1] + lattice[2][0] * coords[2],
        lattice[0][1] * coords[0] + lattice[1][1] * coords[1] + lattice[2][1] * coords[2],
        lattice[0][2] * coords[0] + lattice[1][2] * coords[1] + lattice[2][2] * coords[2],
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

#[derive(Serialize)]
struct AppSnapshotPayload {
    overview: AppOverview,
    data_foundation: DataFoundation,
    data_flow_lab: DataFlowLab,
    database: DatabaseProfile,
    catalog: WorkspaceCatalog,
}

#[derive(Debug, serde::Deserialize)]
struct AutoEmulateSummaryFile {
    ga_run_dir: String,
    campaign_id: String,
    branch_id: String,
    fidelity: String,
    model_variant: String,
    selected_model_name: String,
    feature_family: String,
    feature_version: String,
    feature_count: usize,
    training_observation_count: usize,
    pending_candidate_count: usize,
    incumbent_target: f64,
    best_ranked_candidate_id: Option<String>,
    highest_uncertainty_candidate_id: Option<String>,
    top_acquisition_score: Option<f64>,
    top_uncertainty_score: Option<f64>,
}

#[derive(Debug, serde::Deserialize)]
struct AutoEmulateRequestFile {
    feature_names: Vec<String>,
    #[serde(default)]
    candidate_rows: Vec<AutoEmulateCandidateRequestFile>,
}

#[derive(Debug, serde::Deserialize)]
struct AutoEmulateCandidateRequestFile {
    candidate_id: String,
    #[serde(default)]
    features: Vec<f64>,
    metadata: Option<AutoEmulateCandidateMetadataFile>,
}

#[derive(Debug, serde::Deserialize)]
struct AutoEmulateCandidateMetadataFile {
    label: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct AutoEmulateResponseFile {
    #[serde(default)]
    checkpoint_path: Option<String>,
    predictions: Vec<AutoEmulatePredictionFile>,
}

#[derive(Debug, serde::Deserialize)]
struct AutoEmulatePredictionFile {
    candidate_id: String,
    means: Vec<f64>,
    variances: Vec<f64>,
    acquisition_score: f64,
    uncertainty_score: f64,
    rank: usize,
}

#[derive(Debug, serde::Deserialize)]
struct AutoEmulateReportFile {
    training_target_summary: Option<AutoEmulateScalarSummary>,
    predicted_target_summary: Option<AutoEmulateScalarSummary>,
    predicted_variance_summary: Option<AutoEmulateScalarSummary>,
    acquisition_score_summary: Option<AutoEmulateScalarSummary>,
    uncertainty_score_summary: Option<AutoEmulateScalarSummary>,
    #[serde(default)]
    top_ranked_candidates: Vec<AutoEmulateCandidateDiagnostic>,
    #[serde(default)]
    highest_uncertainty_candidates: Vec<AutoEmulateCandidateDiagnostic>,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use std::collections::BTreeMap;

    use super::{
        connect_data_store_model, load_data_flow_lab_model, load_workspace_catalog_model,
        AppSnapshotPayload, TopologySummaryView,
    };
    use crate::state::AppState;

    #[test]
    fn topology_summary_view_serializes_cleanly() {
        let mut species_histogram = BTreeMap::new();
        species_histogram.insert(2usize, 3usize);

        let summary = TopologySummaryView {
            node_count: 5,
            edge_count: 4,
            connected_components: 1,
            degree_histogram_by_species: BTreeMap::from([("Ti".to_string(), species_histogram)]),
        };

        let json = serde_json::to_value(&summary).expect("serialize topology summary");
        assert!(json.get("degree_histogram_by_species").is_some());
    }

    #[test]
    fn app_snapshot_payload_serializes_cleanly() {
        tauri::async_runtime::block_on(async {
            let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .expect("workspace root")
                .to_path_buf();
            let campaign_checkpoint = workspace_root
                .join("docs")
                .join("active")
                .join("app")
                .join("PATINA_DESKTOP_APP_CAMPAIGN_2026-04-23.md");
            let temp_dir = tempfile::tempdir().expect("tempdir");
            let state = AppState::new(
                temp_dir.path().to_path_buf(),
                workspace_root,
                campaign_checkpoint,
            );

            let payload = AppSnapshotPayload {
                overview: state.overview(),
                data_foundation: state.data_foundation(),
                data_flow_lab: load_data_flow_lab_model(&state).await.expect("data flow"),
                database: connect_data_store_model(&state).await.expect("database"),
                catalog: load_workspace_catalog_model(&state).expect("catalog"),
            };

            serde_json::to_string(&payload).expect("serialize full snapshot");
        });
    }
}
