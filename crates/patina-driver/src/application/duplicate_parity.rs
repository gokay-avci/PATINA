use super::driver_support::{build_external_hashkey, candidate_hashkey_cache_key};
use super::rust_janus_ga::{ScottGaDuplicateTraceRecord, SharedScottDuplicateTrace};
use super::scott_topology_types::AtomSpecRecord;
use anyhow::{anyhow, Context, Result};
use patina_evaluator::ScottDuplicateReason as EvaluatorDuplicateReason;
use patina_search::{
    classify_duplicate_candidates_after_hashkey_probe, DuplicateCandidateSnapshot, DuplicatePolicy,
    ScottDuplicateClassifier, ScottDuplicateReason, ScottGaMember,
};
use patina_types::Candidate;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct CompareDuplicateParityRequest {
    pub run_dir: PathBuf,
    pub pmoi_tolerance: f64,
}

#[derive(Debug, Clone)]
pub struct CompareDuplicateEdgeCasesRequest {
    pub output_dir: PathBuf,
    pub atoms_in_template: Option<PathBuf>,
    pub use_dreadnaut_keys: bool,
    pub hashkey_radius: String,
    pub hashkey_radius_const: f64,
    pub pmoi_tolerance: f64,
}

#[derive(Debug, Clone, Serialize)]
struct DuplicateParityReport {
    run_dir: String,
    classifier: DuplicateParityClassifierSettings,
    summary: DuplicateParitySummary,
    comparisons: Vec<DuplicateParityComparison>,
}

#[derive(Debug, Clone, Serialize)]
struct DuplicateParityClassifierSettings {
    topology_cutoff: f64,
    energy_tolerance: f64,
    pmoi_tolerance: f64,
    enable_pmoi: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
struct DuplicateParitySummary {
    duplicate_events_total: usize,
    comparable_pairs: usize,
    matched_reason_count: usize,
    mismatched_reason_count: usize,
    unresolved_pairs: usize,
    native_reason_counts: BTreeMap<String, usize>,
    rust_reason_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
struct DuplicateParityComparison {
    generation: usize,
    left_id: String,
    right_id: String,
    native_reason: String,
    rust_reason: Option<String>,
    matched: bool,
    left_structure: Option<String>,
    right_structure: Option<String>,
    left_energy: Option<f64>,
    right_energy: Option<f64>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DuplicateEdgeCaseReport {
    generated_at: String,
    output_dir: String,
    classifier: DuplicateParityClassifierSettings,
    cases: Vec<DuplicateEdgeCaseResult>,
}

#[derive(Debug, Clone, Serialize)]
struct DuplicateEdgeCaseResult {
    case_name: String,
    description: String,
    expected_reason: String,
    matched_expectation: bool,
    left_energy: f64,
    right_energy: f64,
    left_hashkey: Option<String>,
    right_hashkey: Option<String>,
    exact_hashkey_match: Option<bool>,
    rust_reason: Option<String>,
    pmoi_left: [f64; 3],
    pmoi_right: [f64; 3],
    pmoi_diff_sum: f64,
}

#[derive(Debug)]
pub(crate) struct ExternalHashkeyDuplicatePolicy {
    classifier: ScottDuplicateClassifier,
    atom_specs: Option<Vec<AtomSpecRecord>>,
    radius_mode: String,
    radius_const: f64,
    hkg_path: PathBuf,
    scratch_dir: PathBuf,
    cache: Mutex<HashMap<u64, Option<String>>>,
    duplicate_trace: SharedScottDuplicateTrace,
}

impl ExternalHashkeyDuplicatePolicy {
    pub(crate) fn new(
        classifier: ScottDuplicateClassifier,
        atom_specs: Option<Vec<AtomSpecRecord>>,
        radius_mode: String,
        radius_const: f64,
        hkg_path: PathBuf,
        scratch_dir: PathBuf,
        duplicate_trace: SharedScottDuplicateTrace,
    ) -> Result<Self> {
        fs::create_dir_all(&scratch_dir).with_context(|| {
            format!(
                "failed to create external hashkey scratch dir `{}`",
                scratch_dir.display()
            )
        })?;
        Ok(Self {
            classifier,
            atom_specs,
            radius_mode,
            radius_const,
            hkg_path,
            scratch_dir,
            cache: Mutex::new(HashMap::new()),
            duplicate_trace,
        })
    }

    fn compare_external_hashkeys(
        &self,
        left: &Candidate,
        right: &Candidate,
    ) -> Result<(Option<String>, Option<String>, Option<ScottDuplicateReason>)> {
        let left_hash = self.hashkey_for_candidate(left)?;
        let right_hash = self.hashkey_for_candidate(right)?;
        if let (Some(left_hash), Some(right_hash)) = (left_hash.as_ref(), right_hash.as_ref()) {
            if left_hash == right_hash {
                return Ok((
                    Some(left_hash.clone()),
                    Some(right_hash.clone()),
                    Some(ScottDuplicateReason::Hashkey),
                ));
            }
        }
        Ok((left_hash, right_hash, None))
    }

    fn hashkey_for_candidate(&self, candidate: &Candidate) -> Result<Option<String>> {
        let cache_key = candidate_hashkey_cache_key(candidate);
        if let Some(cached) = self
            .cache
            .lock()
            .map_err(|_| anyhow!("external hashkey cache mutex poisoned"))?
            .get(&cache_key)
            .cloned()
        {
            return Ok(cached);
        }

        let hash = build_external_hashkey(
            candidate,
            self.atom_specs.as_deref(),
            &self.radius_mode,
            self.radius_const,
            &self.hkg_path,
            &self.scratch_dir,
            &format!("{cache_key:016x}"),
        )?;
        self.cache
            .lock()
            .map_err(|_| anyhow!("external hashkey cache mutex poisoned"))?
            .insert(cache_key, hash.clone());
        Ok(hash)
    }

    fn record_duplicate_trace(
        &self,
        left: &ScottGaMember,
        right: &ScottGaMember,
        reason: ScottDuplicateReason,
        left_hashkey: Option<String>,
        right_hashkey: Option<String>,
        exact_hashkey_match: Option<bool>,
    ) {
        let reason = match reason {
            ScottDuplicateReason::Hashkey => EvaluatorDuplicateReason::Hashkey,
            ScottDuplicateReason::Pmoi => EvaluatorDuplicateReason::Pmoi,
            ScottDuplicateReason::EnergyTol => EvaluatorDuplicateReason::EnergyTolerance,
        };
        let record = ScottGaDuplicateTraceRecord {
            left_source_label: left.source_candidate.label.clone(),
            right_source_label: right.source_candidate.label.clone(),
            left_relaxed_label: left.result.relaxed_candidate.label.clone(),
            right_relaxed_label: right.result.relaxed_candidate.label.clone(),
            left_energy: left.result.energy,
            right_energy: right.result.energy,
            left_hashkey,
            right_hashkey,
            exact_hashkey_match,
            reason,
        };
        match self.duplicate_trace.lock() {
            Ok(mut records) => records.push(record),
            Err(poisoned) => poisoned.into_inner().push(record),
        }
    }
}

impl DuplicatePolicy for ExternalHashkeyDuplicatePolicy {
    fn classify(
        &self,
        left: &ScottGaMember,
        right: &ScottGaMember,
    ) -> Option<ScottDuplicateReason> {
        if !left.is_valid() || !right.is_valid() {
            return None;
        }

        match self.compare_external_hashkeys(
            &left.result.relaxed_candidate,
            &right.result.relaxed_candidate,
        ) {
            Ok((left_hashkey, right_hashkey, Some(reason))) => {
                let resolved_reason = self
                    .classifier
                    .classify_after_hashkey_probe(true, left, right)
                    .unwrap_or(reason);
                self.record_duplicate_trace(
                    left,
                    right,
                    resolved_reason,
                    left_hashkey,
                    right_hashkey,
                    Some(true),
                );
                Some(resolved_reason)
            }
            Ok((Some(left_hashkey), Some(right_hashkey), None)) => {
                let reason = self
                    .classifier
                    .classify_after_hashkey_probe(false, left, right);
                if let Some(reason) = reason {
                    self.record_duplicate_trace(
                        left,
                        right,
                        reason,
                        Some(left_hashkey),
                        Some(right_hashkey),
                        Some(false),
                    );
                }
                reason
            }
            Ok((_left_hashkey, _right_hashkey, None)) => None,
            Err(_error) => None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum RustJanusDuplicatePolicy {
    External(ExternalHashkeyDuplicatePolicy),
    Classifier(ScottDuplicateClassifier),
}

impl DuplicatePolicy for RustJanusDuplicatePolicy {
    fn classify(
        &self,
        left: &ScottGaMember,
        right: &ScottGaMember,
    ) -> Option<ScottDuplicateReason> {
        match self {
            Self::External(policy) => policy.classify(left, right),
            Self::Classifier(policy) => policy.classify(left, right),
        }
    }
}

pub fn compare_duplicate_parity(request: CompareDuplicateParityRequest) -> Result<()> {
    let run_dir = crate::absolutize_path(&request.run_dir)?;
    let raw_dir = run_dir.join("raw");
    let structures_dir = run_dir.join("outputs").join("structures");
    let audit_path = raw_dir.join("hashkey_audit_events.csv");
    if !audit_path.exists() {
        return Err(anyhow!(
            "native audit file not found at `{}`",
            audit_path.display()
        ));
    }

    let structure_index = super::scott_topology_export::index_structure_snapshots(&structures_dir)?;
    let energy_index = collect_energy_index(&raw_dir)?;
    let native_workdir = parse_native_workdir_from_manifest(&run_dir)?;
    let atom_specs = super::scott_topology_export::parse_atoms_file(&raw_dir.join("atoms.in"))?;
    let run_job_text = fs::read_to_string(raw_dir.join("run.job")).unwrap_or_default();
    let radius_mode =
        parse_run_job_value(&run_job_text, "C_HASHKEY_RADIUS").unwrap_or_else(|| "IR".to_string());
    let radius_const = parse_run_job_value(&run_job_text, "HASHKEY_RADIUS_CONST")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.4);
    let hkg_path = parse_run_job_value(&run_job_text, "HKG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(patina_dreadnaut::bundled_dreadnaut_path);
    let classifier =
        ScottDuplicateClassifier::pmoi_energy_fallback_with_pmoi_tolerance(request.pmoi_tolerance);
    let mut summary = DuplicateParitySummary::default();
    let mut comparisons = Vec::new();

    for event in super::scott_topology_export::parse_topology_audit_events(&audit_path)? {
        if event.event_kind != "duplicate" || event.left_id.is_empty() || event.right_id.is_empty()
        {
            continue;
        }
        summary.duplicate_events_total += 1;
        *summary
            .native_reason_counts
            .entry(event.detail.clone())
            .or_insert(0) += 1;

        let left_structure = resolve_native_event_structure(
            native_workdir.as_deref(),
            event.ga_iter,
            &event.left_id,
            &structure_index,
        );
        let right_structure = resolve_native_event_structure(
            native_workdir.as_deref(),
            event.ga_iter,
            &event.right_id,
            &structure_index,
        );
        let mut notes = Vec::new();
        let rust_reason = match (&left_structure, &right_structure) {
            (Some(left_path), Some(right_path)) => {
                let left_candidate =
                    super::scott_topology_export::parse_candidate_snapshot(left_path)?;
                let right_candidate =
                    super::scott_topology_export::parse_candidate_snapshot(right_path)?;
                let left_energy = energy_index
                    .get(&event.left_id)
                    .copied()
                    .unwrap_or(f64::INFINITY);
                let right_energy = energy_index
                    .get(&event.right_id)
                    .copied()
                    .unwrap_or(f64::INFINITY);
                summary.comparable_pairs += 1;
                classify_duplicate_parity_reason(
                    &left_candidate,
                    left_energy,
                    &right_candidate,
                    right_energy,
                    &classifier,
                    Some(&atom_specs),
                    &radius_mode,
                    radius_const,
                    &hkg_path,
                    &raw_dir,
                )?
            }
            _ => {
                summary.unresolved_pairs += 1;
                if left_structure.is_none() {
                    notes.push(format!("missing structure for {}", event.left_id));
                }
                if right_structure.is_none() {
                    notes.push(format!("missing structure for {}", event.right_id));
                }
                None
            }
        };

        if let Some(reason) = rust_reason.as_ref() {
            *summary
                .rust_reason_counts
                .entry(reason.clone())
                .or_insert(0) += 1;
        }
        let matched = rust_reason.as_deref() == Some(event.detail.as_str());
        if rust_reason.is_some() {
            if matched {
                summary.matched_reason_count += 1;
            } else {
                summary.mismatched_reason_count += 1;
            }
        }

        comparisons.push(DuplicateParityComparison {
            generation: event.ga_iter,
            left_id: event.left_id.clone(),
            right_id: event.right_id.clone(),
            native_reason: event.detail.clone(),
            rust_reason,
            matched,
            left_structure: left_structure.map(|path| path.display().to_string()),
            right_structure: right_structure.map(|path| path.display().to_string()),
            left_energy: energy_index.get(&event.left_id).copied(),
            right_energy: energy_index.get(&event.right_id).copied(),
            notes,
        });
    }

    let report = DuplicateParityReport {
        run_dir: run_dir.display().to_string(),
        classifier: DuplicateParityClassifierSettings {
            topology_cutoff: classifier.topology_cutoff,
            energy_tolerance: classifier.energy_tolerance,
            pmoi_tolerance: classifier.pmoi_tolerance,
            enable_pmoi: classifier.enable_pmoi,
        },
        summary,
        comparisons,
    };
    let output_path = raw_dir.join("duplicate_parity_report.json");
    fs::write(&output_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write `{}`", output_path.display()))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn compare_duplicate_edge_cases(request: CompareDuplicateEdgeCasesRequest) -> Result<()> {
    let output_dir = crate::absolutize_path(&request.output_dir)?;
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create `{}`", output_dir.display()))?;
    if !request.use_dreadnaut_keys {
        return Err(anyhow!(
            "duplicate edge-case comparison requires --use-dreadnaut-keys true"
        ));
    }
    let atom_specs = request
        .atoms_in_template
        .as_ref()
        .map(|path| crate::absolutize_path(path))
        .transpose()?
        .as_deref()
        .map(super::scott_topology_export::parse_atoms_file)
        .transpose()?;
    let hkg_path = patina_dreadnaut::bundled_dreadnaut_path();
    let classifier =
        ScottDuplicateClassifier::pmoi_energy_fallback_with_pmoi_tolerance(request.pmoi_tolerance);
    let cases = synthetic_duplicate_edge_cases();
    let mut results = Vec::with_capacity(cases.len());

    for (idx, case) in cases.into_iter().enumerate() {
        let left_hash = build_external_hashkey(
            &case.left,
            atom_specs.as_deref(),
            &request.hashkey_radius,
            request.hashkey_radius_const,
            &hkg_path,
            &output_dir,
            &format!("{idx:02}_left"),
        )?;
        let right_hash = build_external_hashkey(
            &case.right,
            atom_specs.as_deref(),
            &request.hashkey_radius,
            request.hashkey_radius_const,
            &hkg_path,
            &output_dir,
            &format!("{idx:02}_right"),
        )?;
        let exact_hashkey_match = match (&left_hash, &right_hash) {
            (Some(left), Some(right)) => Some(left == right),
            _ => None,
        };
        let rust_reason = classify_duplicate_parity_reason(
            &case.left,
            case.left_energy,
            &case.right,
            case.right_energy,
            &classifier,
            atom_specs.as_deref(),
            &request.hashkey_radius,
            request.hashkey_radius_const,
            &hkg_path,
            &output_dir,
        )?;
        let expected_reason = case.expected_reason.unwrap_or("NONE").to_string();
        let matched_expectation = rust_reason.as_deref().unwrap_or("NONE") == expected_reason;
        results.push(DuplicateEdgeCaseResult {
            case_name: case.name.to_string(),
            description: case.description.to_string(),
            expected_reason,
            matched_expectation,
            left_energy: case.left_energy,
            right_energy: case.right_energy,
            left_hashkey: left_hash,
            right_hashkey: right_hash,
            exact_hashkey_match,
            rust_reason,
            pmoi_left: patina_search::scott_intent_pmoi(&case.left),
            pmoi_right: patina_search::scott_intent_pmoi(&case.right),
            pmoi_diff_sum: patina_search::scott_intent_pmoi_difference(&case.left, &case.right),
        });
    }

    let report = DuplicateEdgeCaseReport {
        generated_at: chrono_like_timestamp(),
        output_dir: output_dir.display().to_string(),
        classifier: DuplicateParityClassifierSettings {
            topology_cutoff: classifier.topology_cutoff,
            energy_tolerance: classifier.energy_tolerance,
            pmoi_tolerance: classifier.pmoi_tolerance,
            enable_pmoi: classifier.enable_pmoi,
        },
        cases: results,
    };
    let output_path = output_dir.join("duplicate_edge_case_report.json");
    fs::write(&output_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write `{}`", output_path.display()))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[derive(Debug, Clone)]
struct SyntheticDuplicateEdgeCase {
    name: &'static str,
    description: &'static str,
    expected_reason: Option<&'static str>,
    left: Candidate,
    left_energy: f64,
    right: Candidate,
    right_energy: f64,
}

fn synthetic_duplicate_edge_cases() -> Vec<SyntheticDuplicateEdgeCase> {
    let base = synthetic_candidate(
        "base",
        &["Mg", "O", "Mg", "O"],
        &[
            [0.0, 0.0, 0.0],
            [1.8, 0.0, 0.0],
            [0.0, 1.8, 0.0],
            [1.8, 1.8, 0.0],
        ],
    );
    let translated = translate_candidate(&base, [4.0, -3.5, 1.25], "translated");
    let rotated = rotate_z_candidate(&base, std::f64::consts::FRAC_PI_3, "rotated");
    let permuted = permute_candidate(&base, &[2, 3, 0, 1], "permuted");
    let placeholder = with_placeholder_x(&base, [9.0, 9.0, 9.0], "placeholder_x");
    let periodic = Candidate::periodic(
        "periodic_left",
        vec!["Mg".to_string(), "O".to_string()],
        vec![[0.1, 0.0, 0.0], [1.9, 0.0, 0.0]],
        [[2.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 6.0]],
        [true, false, false],
    );
    let periodic_wrapped = Candidate::periodic(
        "periodic_right",
        vec!["Mg".to_string(), "O".to_string()],
        vec![[0.1, 0.0, 0.0], [-0.1, 0.0, 0.0]],
        [[2.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 6.0]],
        [true, false, false],
    );
    let pmoi_left = synthetic_candidate(
        "pmoi_left",
        &["Mg", "Mg", "Mg", "Mg"],
        &[
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 2.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
    );
    let pmoi_right = synthetic_candidate(
        "pmoi_right",
        &["Mg", "Mg", "Mg", "Mg"],
        &[
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 2.0, 0.0],
            [1.0, 2.0, 0.0],
        ],
    );
    let energy_left = base.clone();
    let energy_right = synthetic_candidate(
        "energy_only",
        &["Mg", "O", "Mg", "O"],
        &[
            [0.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [0.0, 5.0, 0.0],
            [5.0, 5.0, 0.0],
        ],
    );
    let species_swap = synthetic_candidate(
        "species_swap",
        &["O", "Mg", "O", "Mg"],
        &base.fractional_coords,
    );
    let distinct_far = synthetic_candidate(
        "distinct_far",
        &["Mg", "O", "Mg", "O"],
        &[
            [0.0, 0.0, 0.0],
            [6.0, 0.0, 0.0],
            [0.0, 6.0, 0.0],
            [6.0, 6.0, 0.0],
        ],
    );
    let tri_linear = synthetic_candidate(
        "tri_linear",
        &["Mg", "O", "Mg"],
        &[[0.0, 0.0, 0.0], [1.7, 0.0, 0.0], [3.4, 0.0, 0.0]],
    );
    let tri_linear_permuted = permute_candidate(&tri_linear, &[2, 1, 0], "tri_linear_permuted");
    let tri_bent = synthetic_candidate(
        "tri_bent",
        &["Mg", "O", "Mg"],
        &[[0.0, 0.0, 0.0], [1.7, 0.0, 0.0], [2.7, 1.2, 0.0]],
    );
    let tri_bent_mirrored = synthetic_candidate(
        "tri_bent_mirrored",
        &["Mg", "O", "Mg"],
        &[[0.0, 0.0, 0.0], [1.7, 0.0, 0.0], [2.7, -1.2, 0.0]],
    );
    let tetra = synthetic_candidate(
        "tetra",
        &["Mg", "O", "Mg", "O"],
        &[
            [1.0, 1.0, 1.0],
            [-1.0, -1.0, 1.0],
            [-1.0, 1.0, -1.0],
            [1.0, -1.0, -1.0],
        ],
    );
    let tetra_rotated = rotate_z_candidate(&tetra, std::f64::consts::FRAC_PI_4, "tetra_rotated");
    let octa = synthetic_candidate(
        "octa",
        &["Mg", "O", "Mg", "O", "Mg", "O"],
        &[
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ],
    );
    let octa_permuted = permute_candidate(&octa, &[5, 4, 3, 2, 1, 0], "octa_permuted");
    let ring6 = synthetic_candidate(
        "ring6",
        &["Mg", "O", "Mg", "O", "Mg", "O"],
        &[
            [2.0, 0.0, 0.0],
            [1.0, 1.732, 0.0],
            [-1.0, 1.732, 0.0],
            [-2.0, 0.0, 0.0],
            [-1.0, -1.732, 0.0],
            [1.0, -1.732, 0.0],
        ],
    );
    let ring6_rotated = rotate_z_candidate(&ring6, std::f64::consts::FRAC_PI_6, "ring6_rotated");
    let asymmetric_colored = synthetic_candidate(
        "asymmetric_colored",
        &["Mg", "Mg", "O", "O", "Mg"],
        &[
            [0.0, 0.0, 0.0],
            [1.6, 0.0, 0.2],
            [0.5, 1.4, 0.0],
            [2.3, 1.8, 0.1],
            [1.2, 2.9, 0.4],
        ],
    );
    let asymmetric_species_swap = synthetic_candidate(
        "asymmetric_species_swap",
        &["Mg", "O", "Mg", "O", "Mg"],
        &asymmetric_colored.fractional_coords,
    );
    let linear4_short = synthetic_candidate(
        "linear4_short",
        &["Mg", "Mg", "Mg", "Mg"],
        &[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
        ],
    );
    let linear4_long = synthetic_candidate(
        "linear4_long",
        &["Mg", "Mg", "Mg", "Mg"],
        &[
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [6.0, 0.0, 0.0],
        ],
    );
    let trigonal_bipyramid = synthetic_candidate(
        "trigonal_bipyramid",
        &["Mg", "O", "Mg", "O", "Mg"],
        &[
            [0.0, 0.0, 1.5],
            [0.0, 0.0, -1.5],
            [1.5, 0.0, 0.0],
            [-0.75, 1.299, 0.0],
            [-0.75, -1.299, 0.0],
        ],
    );
    let square_pyramid = synthetic_candidate(
        "square_pyramid",
        &["Mg", "O", "Mg", "O", "Mg"],
        &[
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [0.0, 0.0, 1.5],
        ],
    );

    vec![
        SyntheticDuplicateEdgeCase {
            name: "exact_same",
            description: "Same structure and ordering.",
            expected_reason: Some("HASHKEY"),
            left: base.clone(),
            left_energy: -10.0,
            right: base.clone(),
            right_energy: -10.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "translation_invariant",
            description: "Rigid translation should preserve native hashkey identity.",
            expected_reason: Some("HASHKEY"),
            left: base.clone(),
            left_energy: -10.0,
            right: translated,
            right_energy: -9.9,
        },
        SyntheticDuplicateEdgeCase {
            name: "rotation_invariant",
            description: "Rigid rotation should preserve native hashkey identity.",
            expected_reason: Some("HASHKEY"),
            left: base.clone(),
            left_energy: -10.0,
            right: rotated,
            right_energy: -9.8,
        },
        SyntheticDuplicateEdgeCase {
            name: "permuted_atom_order",
            description: "Atom ordering permutation should not change canonical hashkey.",
            expected_reason: Some("HASHKEY"),
            left: base.clone(),
            left_energy: -10.0,
            right: permuted,
            right_energy: -10.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "placeholder_filtered",
            description: "Placeholder X atom should be ignored by native graph generation.",
            expected_reason: Some("HASHKEY"),
            left: base.clone(),
            left_energy: -10.0,
            right: placeholder,
            right_energy: -10.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "periodic_wrap_equivalent",
            description: "Minimum-image wrapping should preserve connectivity under periodic lattice lengths.",
            expected_reason: Some("HASHKEY"),
            left: periodic,
            left_energy: -1.0,
            right: periodic_wrapped,
            right_energy: -1.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "pmoi_same_different_graph",
            description: "Same normalized PMOI but different connectivity should fall through to PMOI duplicate classification.",
            expected_reason: Some("PMOI"),
            left: pmoi_left,
            left_energy: -4.0,
            right: pmoi_right,
            right_energy: 12.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "energy_tolerance_only",
            description: "Distinct topology and PMOI with near-identical energies should only match on energy tolerance.",
            expected_reason: Some("ENERGY_TOL"),
            left: energy_left,
            left_energy: -8.000,
            right: energy_right,
            right_energy: -8.005,
        },
        SyntheticDuplicateEdgeCase {
            name: "species_partition_sensitive",
            description: "Species reassignment on the same coordinates should change the native color partition and avoid duplicate classification.",
            expected_reason: None,
            left: base.clone(),
            left_energy: -10.0,
            right: species_swap,
            right_energy: -10.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "clearly_distinct",
            description: "Far-separated topology and large energy gap should not be classified as duplicates.",
            expected_reason: None,
            left: base,
            left_energy: -10.0,
            right: distinct_far,
            right_energy: 3.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "triatomic_linear_permutation",
            description: "Three-atom linear chain should keep the same native hashkey under atom-order reversal.",
            expected_reason: Some("HASHKEY"),
            left: tri_linear,
            left_energy: -2.0,
            right: tri_linear_permuted,
            right_energy: -2.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "triatomic_bent_mirror",
            description: "Mirroring a bent triatomic cluster should preserve identity after canonical graph labeling.",
            expected_reason: Some("HASHKEY"),
            left: tri_bent,
            left_energy: -2.1,
            right: tri_bent_mirrored,
            right_energy: -2.1,
        },
        SyntheticDuplicateEdgeCase {
            name: "tetrahedral_rotation",
            description: "A tetrahedral 4-atom cluster should remain hashkey-identical under rigid rotation.",
            expected_reason: Some("HASHKEY"),
            left: tetra,
            left_energy: -6.0,
            right: tetra_rotated,
            right_energy: -6.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "octahedral_permutation",
            description: "A six-atom octahedral cluster should preserve canonical identity under heavy permutation.",
            expected_reason: Some("HASHKEY"),
            left: octa,
            left_energy: -9.0,
            right: octa_permuted,
            right_energy: -9.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "hexagonal_ring_rotation",
            description: "A six-atom ring should preserve native hashkey identity under rigid rotation.",
            expected_reason: Some("HASHKEY"),
            left: ring6,
            left_energy: -8.0,
            right: ring6_rotated,
            right_energy: -8.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "asymmetric_species_partition_swap",
            description: "Changing species assignments on an asymmetric 5-atom cluster should alter the color partition and avoid duplicate identity.",
            expected_reason: None,
            left: asymmetric_colored,
            left_energy: -4.0,
            right: asymmetric_species_swap,
            right_energy: -4.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "linear_scaling_pmoi_probe",
            description: "Uniformly stretched linear chains probe whether PMOI is broad enough to collapse scale-separated structures.",
            expected_reason: Some("PMOI"),
            left: linear4_short,
            left_energy: -3.0,
            right: linear4_long,
            right_energy: 7.0,
        },
        SyntheticDuplicateEdgeCase {
            name: "five_atom_shape_competition",
            description: "Trigonal bipyramidal and square-pyramidal 5-atom clusters should expose PMOI versus topology behavior on distinct shapes.",
            expected_reason: None,
            left: trigonal_bipyramid,
            left_energy: -5.5,
            right: square_pyramid,
            right_energy: -5.4,
        },
    ]
}

fn synthetic_candidate(label: &str, species: &[&str], fractional_coords: &[[f64; 3]]) -> Candidate {
    Candidate::cluster(
        label,
        species.iter().map(|entry| (*entry).to_string()).collect(),
        fractional_coords.to_vec(),
    )
}

fn translate_candidate(candidate: &Candidate, delta: [f64; 3], label: &str) -> Candidate {
    let mut out = candidate.clone();
    out.label = label.to_string();
    for coord in &mut out.fractional_coords {
        coord[0] += delta[0];
        coord[1] += delta[1];
        coord[2] += delta[2];
    }
    out
}

fn rotate_z_candidate(candidate: &Candidate, angle: f64, label: &str) -> Candidate {
    let mut out = candidate.clone();
    out.label = label.to_string();
    let center = center_of_geometry(&out.fractional_coords);
    let cos = angle.cos();
    let sin = angle.sin();
    for coord in &mut out.fractional_coords {
        let x = coord[0] - center[0];
        let y = coord[1] - center[1];
        coord[0] = center[0] + cos * x - sin * y;
        coord[1] = center[1] + sin * x + cos * y;
    }
    out
}

fn permute_candidate(candidate: &Candidate, order: &[usize], label: &str) -> Candidate {
    Candidate::from_parts(
        label,
        order
            .iter()
            .map(|&idx| candidate.species[idx].clone())
            .collect(),
        order
            .iter()
            .map(|&idx| candidate.fractional_coords[idx])
            .collect(),
        candidate.lattice,
        candidate.periodic_axes,
    )
}

fn with_placeholder_x(candidate: &Candidate, x_coord: [f64; 3], label: &str) -> Candidate {
    let mut out = candidate.clone();
    out.species.push("X".into());
    out.fractional_coords.push(x_coord);
    out.label = label.to_string();
    out
}

fn center_of_geometry(coords: &[[f64; 3]]) -> [f64; 3] {
    let n = coords.len().max(1) as f64;
    let mut center = [0.0; 3];
    for row in coords {
        center[0] += row[0];
        center[1] += row[1];
        center[2] += row[2];
    }
    center[0] /= n;
    center[1] /= n;
    center[2] /= n;
    center
}

fn chrono_like_timestamp() -> String {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

fn parse_native_workdir_from_manifest(run_dir: &Path) -> Result<Option<PathBuf>> {
    let manifest_path = run_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read `{}`", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse `{}`", manifest_path.display()))?;
    Ok(value
        .get("provenance")
        .and_then(|node| node.get("workdir"))
        .and_then(|node| node.as_str())
        .map(PathBuf::from))
}

fn resolve_native_event_structure(
    native_workdir: Option<&Path>,
    generation: usize,
    cluster_id: &str,
    structure_index: &HashMap<String, PathBuf>,
) -> Option<PathBuf> {
    if let Some(workdir) = native_workdir {
        let candidate = workdir
            .join("run")
            .join(generation.to_string())
            .join(format!("{cluster_id}.xyz"));
        if candidate.exists() {
            return Some(candidate);
        }
        return None;
    }
    structure_index.get(cluster_id).cloned()
}

#[allow(clippy::too_many_arguments)]
fn classify_duplicate_parity_reason(
    left: &Candidate,
    left_energy: f64,
    right: &Candidate,
    right_energy: f64,
    classifier: &ScottDuplicateClassifier,
    atom_specs: Option<&[AtomSpecRecord]>,
    radius_mode: &str,
    radius_const: f64,
    hkg_path: &Path,
    scratch_dir: &Path,
) -> Result<Option<String>> {
    let left_hash = build_external_hashkey(
        left,
        atom_specs,
        radius_mode,
        radius_const,
        hkg_path,
        scratch_dir,
        "left",
    )?;
    let right_hash = build_external_hashkey(
        right,
        atom_specs,
        radius_mode,
        radius_const,
        hkg_path,
        scratch_dir,
        "right",
    )?;
    if let (Some(left_hash), Some(right_hash)) = (&left_hash, &right_hash) {
        return Ok(classify_duplicate_candidates_after_hashkey_probe(
            left_hash == right_hash,
            DuplicateCandidateSnapshot {
                candidate: left,
                energy: left_energy,
                converged: true,
            },
            DuplicateCandidateSnapshot {
                candidate: right,
                energy: right_energy,
                converged: true,
            },
            classifier,
        )
        .map(|reason| reason.as_str().to_string()));
    }

    Ok(classify_duplicate_candidates_after_hashkey_probe(
        false,
        DuplicateCandidateSnapshot {
            candidate: left,
            energy: left_energy,
            converged: true,
        },
        DuplicateCandidateSnapshot {
            candidate: right,
            energy: right_energy,
            converged: true,
        },
        classifier,
    )
    .map(|reason| reason.as_str().to_string()))
}

fn parse_run_job_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        let (lhs, rhs) = trimmed.split_once(':')?;
        if lhs.trim() != key {
            return None;
        }
        Some(rhs.trim().trim_matches('\'').to_string())
    })
}

fn collect_energy_index(raw_dir: &Path) -> Result<HashMap<String, f64>> {
    let mut index = HashMap::new();
    let mut ga_paths = fs::read_dir(raw_dir)
        .with_context(|| format!("failed to read `{}`", raw_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("gaStatistics") && name.ends_with(".csv"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    ga_paths.sort();
    for path in ga_paths {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read `{}`", path.display()))?;
        for line in text.lines().skip(1) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let parts = trimmed.split(',').map(str::trim).collect::<Vec<_>>();
            if parts.len() < 6 {
                continue;
            }
            let cluster_id = parts[1];
            let Ok(energy) = parts[5].parse::<f64>() else {
                continue;
            };
            index.insert(cluster_id.to_string(), energy);
        }
    }
    Ok(index)
}
