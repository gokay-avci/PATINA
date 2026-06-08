use std::collections::BTreeMap;

use serde::Serialize;

use super::ga_execution::{RustJanusGenerationArtifact, RustJanusOriginMetricRow};

#[derive(Debug, Clone, Serialize)]
pub struct BatchEvaluationSummary {
    pub backend: crate::EvalBackendKind,
    pub runner_mode: crate::RunnerMode,
    pub workers: usize,
    pub keep_dirs: bool,
    pub workdir: std::path::PathBuf,
    pub backend_metadata: Option<serde_json::Value>,
    pub telemetry: BatchEvaluationTelemetry,
    pub responses: Vec<patina_types::WorkerResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerationTelemetry {
    pub generation: usize,
    pub request_count: usize,
    pub worker_count: usize,
    pub elapsed_secs: f64,
    pub success_count: usize,
    pub failure_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerationRunSummary {
    pub backend: crate::EvalBackendKind,
    pub runner_mode: crate::RunnerMode,
    pub workers: usize,
    pub generations: usize,
    pub candidate_count: usize,
    pub keep_dirs: bool,
    pub workdir: std::path::PathBuf,
    pub backend_metadata: Option<serde_json::Value>,
    pub telemetry: BatchEvaluationTelemetry,
    pub generation_telemetry: Vec<GenerationTelemetry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CampaignGenerationArtifact {
    pub generation: usize,
    pub request_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub converged_count: usize,
    pub elapsed_secs: f64,
    pub best_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub worst_energy: Option<f64>,
    pub unique_species_profiles: Option<usize>,
    pub unique_edge_profiles: Option<usize>,
    pub unique_coordination_profiles: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CampaignResult {
    pub summary: GenerationRunSummary,
    pub generations: Vec<CampaignGenerationArtifact>,
    pub generation_responses: Vec<Vec<patina_types::WorkerResponse>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchEvaluationTelemetry {
    pub request_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub wall_time_sum_secs: f64,
    pub elapsed_secs: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerationFailureSummary {
    pub generation: usize,
    pub phase: String,
    pub failure_count: usize,
    pub failure_kind_counts: BTreeMap<String, usize>,
    pub sample_messages_by_kind: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RustJanusSearchSummary {
    pub workflow_id: &'static str,
    pub workflow_owner: &'static str,
    pub backend: &'static str,
    pub lane_mode: String,
    pub duplicate_policy_mode: String,
    pub duplicate_filter_stack: Vec<crate::application::filter_taxonomy::FilterDescriptor>,
    pub workers: usize,
    pub requested_generations: usize,
    pub population_size: usize,
    pub seed: u64,
    pub system: String,
    pub telemetry: BatchEvaluationTelemetry,
    pub generations: Vec<RustJanusGenerationArtifact>,
    pub origin_metrics: Vec<RustJanusOriginMetricRow>,
    pub janus_guardrails: crate::application::janus_guardrails::JanusGuardrailReport,
    pub operator_policy: crate::application::janus_operator_policy::JanusGaOperatorPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaEmulateSummaryReport {
    pub workflow_owner: &'static str,
    pub influence_mode: &'static str,
    pub ga_run_dir: std::path::PathBuf,
    pub surrogate_artifact_dir: std::path::PathBuf,
    pub campaign_id: String,
    pub branch_id: String,
    pub objective: patina_emulate::LearningObjective,
    pub fidelity: patina_emulate::FidelityClass,
    pub model_variant: patina_emulate::SurrogateModelVariant,
    pub selected_model_name: String,
    pub feature_family: String,
    pub feature_version: String,
    pub feature_count: usize,
    pub training_observation_count: usize,
    pub pending_candidate_count: usize,
    pub incumbent_target: f64,
    pub best_ranked_candidate_id: Option<String>,
    pub highest_uncertainty_candidate_id: Option<String>,
    pub top_acquisition_score: Option<f64>,
    pub top_uncertainty_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScalarMetricSummary {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaEmulateCandidateDiagnosticRow {
    pub candidate_id: String,
    pub label: Option<String>,
    pub rank: usize,
    pub predicted_mean: f64,
    pub predicted_variance: f64,
    pub acquisition_score: f64,
    pub uncertainty_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaEmulateReport {
    pub workflow_owner: &'static str,
    pub influence_mode: &'static str,
    pub ga_run_dir: std::path::PathBuf,
    pub surrogate_artifact_dir: std::path::PathBuf,
    pub campaign_id: String,
    pub branch_id: String,
    pub objective: patina_emulate::LearningObjective,
    pub fidelity: patina_emulate::FidelityClass,
    pub model_variant: patina_emulate::SurrogateModelVariant,
    pub selected_model_name: String,
    pub feature_family: String,
    pub feature_version: String,
    pub feature_count: usize,
    pub training_observation_count: usize,
    pub pending_candidate_count: usize,
    pub incumbent_target: f64,
    pub training_target_summary: Option<ScalarMetricSummary>,
    pub predicted_target_summary: Option<ScalarMetricSummary>,
    pub predicted_variance_summary: Option<ScalarMetricSummary>,
    pub acquisition_score_summary: Option<ScalarMetricSummary>,
    pub uncertainty_score_summary: Option<ScalarMetricSummary>,
    pub top_ranked_candidates: Vec<GaEmulateCandidateDiagnosticRow>,
    pub highest_uncertainty_candidates: Vec<GaEmulateCandidateDiagnosticRow>,
}

pub fn summarize_batch_responses(
    responses: &[patina_types::WorkerResponse],
    elapsed_secs: f64,
) -> BatchEvaluationTelemetry {
    let mut success_count = 0usize;
    let mut failure_count = 0usize;
    let mut wall_time_sum_secs = 0.0f64;

    for response in responses {
        match &response.outcome {
            patina_types::WorkerOutcome::Success { result } => {
                success_count += 1;
                wall_time_sum_secs += result.wall_time.as_secs_f64();
            }
            patina_types::WorkerOutcome::Failure { .. } => {
                failure_count += 1;
            }
        }
    }

    BatchEvaluationTelemetry {
        request_count: responses.len(),
        success_count,
        failure_count,
        wall_time_sum_secs,
        elapsed_secs,
    }
}

pub fn summarize_failure_kinds(
    responses: &[patina_types::WorkerResponse],
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for response in responses {
        if let patina_types::WorkerOutcome::Failure { error } = &response.outcome {
            *counts
                .entry(worker_failure_kind_label(&error.kind).to_string())
                .or_insert(0) += 1;
        }
    }
    counts
}

pub fn build_generation_failure_summary(
    generation: usize,
    phase: &str,
    responses: &[patina_types::WorkerResponse],
) -> GenerationFailureSummary {
    let mut failure_kind_counts = BTreeMap::new();
    let mut sample_messages_by_kind: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for response in responses {
        if let patina_types::WorkerOutcome::Failure { error } = &response.outcome {
            let key = worker_failure_kind_label(&error.kind).to_string();
            *failure_kind_counts.entry(key.clone()).or_insert(0) += 1;
            let samples = sample_messages_by_kind.entry(key).or_default();
            if samples.len() < 3 && !samples.iter().any(|existing| existing == &error.message) {
                samples.push(error.message.clone());
            }
        }
    }

    let failure_count = failure_kind_counts.values().copied().sum();
    GenerationFailureSummary {
        generation,
        phase: phase.to_string(),
        failure_count,
        failure_kind_counts,
        sample_messages_by_kind,
    }
}

pub fn build_generation_telemetry(
    generation_index: usize,
    worker_count: usize,
    elapsed_secs: f64,
    responses: &[patina_types::WorkerResponse],
) -> GenerationTelemetry {
    let summary = summarize_batch_responses(responses, elapsed_secs);
    GenerationTelemetry {
        generation: generation_index,
        request_count: summary.request_count,
        worker_count,
        elapsed_secs: summary.elapsed_secs,
        success_count: summary.success_count,
        failure_count: summary.failure_count,
    }
}

pub fn build_campaign_generation_artifact(
    generation_index: usize,
    elapsed_secs: f64,
    responses: &[patina_types::WorkerResponse],
) -> CampaignGenerationArtifact {
    let mut energies = Vec::new();
    let mut converged_count = 0usize;

    for response in responses {
        if let patina_types::WorkerOutcome::Success { result } = &response.outcome {
            energies.push(result.energy);
            if result.converged {
                converged_count += 1;
            }
        }
    }

    let (best_energy, mean_energy, worst_energy) = summarize_energy_stats(&energies);

    CampaignGenerationArtifact {
        generation: generation_index,
        request_count: responses.len(),
        success_count: energies.len(),
        failure_count: responses.len().saturating_sub(energies.len()),
        converged_count,
        elapsed_secs,
        best_energy,
        mean_energy,
        worst_energy,
        unique_species_profiles: None,
        unique_edge_profiles: None,
        unique_coordination_profiles: None,
    }
}

pub fn summarize_energy_stats(energies: &[f64]) -> (Option<f64>, Option<f64>, Option<f64>) {
    if energies.is_empty() {
        return (None, None, None);
    }

    let mut best = f64::INFINITY;
    let mut worst = f64::NEG_INFINITY;
    let mut total = 0.0;
    for energy in energies {
        best = best.min(*energy);
        worst = worst.max(*energy);
        total += *energy;
    }

    (Some(best), Some(total / energies.len() as f64), Some(worst))
}

fn worker_failure_kind_label(kind: &patina_types::WorkerFailureKind) -> &'static str {
    match kind {
        patina_types::WorkerFailureKind::ProcessFailed => "process_failed",
        patina_types::WorkerFailureKind::ParseFailed => "parse_failed",
        patina_types::WorkerFailureKind::NotConverged => "not_converged",
        patina_types::WorkerFailureKind::Timeout => "timeout",
        patina_types::WorkerFailureKind::Io => "io",
        patina_types::WorkerFailureKind::TemplateInvalid => "template_invalid",
        patina_types::WorkerFailureKind::Internal => "internal",
    }
}

#[cfg(test)]
mod tests {
    use super::{build_generation_failure_summary, summarize_failure_kinds};
    use patina_types::{WorkerFailure, WorkerFailureKind, WorkerOutcome, WorkerResponse};

    fn failure_response(
        request_id: &str,
        kind: WorkerFailureKind,
        message: &str,
    ) -> WorkerResponse {
        WorkerResponse {
            request_id: request_id.to_string(),
            generation: Some(3),
            worker_slot: None,
            outcome: WorkerOutcome::Failure {
                error: WorkerFailure {
                    kind,
                    message: message.to_string(),
                },
            },
        }
    }

    #[test]
    fn summarize_failure_kinds_groups_worker_failures() {
        let responses = vec![
            failure_response("a", WorkerFailureKind::ProcessFailed, "exit 1"),
            failure_response("b", WorkerFailureKind::ProcessFailed, "exit 2"),
            failure_response("c", WorkerFailureKind::ParseFailed, "bad gout"),
        ];

        let counts = summarize_failure_kinds(&responses);

        assert_eq!(counts.get("process_failed"), Some(&2));
        assert_eq!(counts.get("parse_failed"), Some(&1));
    }

    #[test]
    fn generation_failure_summary_keeps_small_message_samples() {
        let responses = vec![
            failure_response("a", WorkerFailureKind::TemplateInvalid, "bad Master.gin"),
            failure_response("b", WorkerFailureKind::TemplateInvalid, "bad Master.gin"),
            failure_response("c", WorkerFailureKind::TemplateInvalid, "atom mismatch"),
            failure_response("d", WorkerFailureKind::TemplateInvalid, "missing marker"),
            failure_response("e", WorkerFailureKind::TemplateInvalid, "extra detail"),
        ];

        let summary = build_generation_failure_summary(3, "evolve", &responses);

        assert_eq!(summary.failure_count, 5);
        assert_eq!(
            summary.failure_kind_counts.get("template_invalid"),
            Some(&5)
        );
        assert_eq!(
            summary.sample_messages_by_kind.get("template_invalid"),
            Some(&vec![
                "bad Master.gin".to_string(),
                "atom mismatch".to_string(),
                "missing marker".to_string()
            ])
        );
    }
}
