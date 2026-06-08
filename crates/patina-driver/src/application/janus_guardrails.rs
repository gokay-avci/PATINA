use super::evaluator_profile::{EvaluatorFamily, SearchOperationProfile};
use super::policy::SearchFamily;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct JanusGuardrailGenerationInput<'a> {
    pub generation: usize,
    pub phase: &'a str,
    pub request_count: usize,
    pub converged_count: usize,
    pub population_size: usize,
    pub duplicate_count: usize,
    pub repopulated_count: usize,
}

#[derive(Debug, Clone)]
pub struct JanusOriginMetricInput<'a> {
    pub generation: usize,
    pub phase: &'a str,
    pub origin: &'a str,
    pub request_count: usize,
    pub converged_count: usize,
    pub survivor_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct JanusGuardrailGenerationReport {
    pub generation: usize,
    pub phase: String,
    pub duplicate_rate_vs_requests: f64,
    pub repopulation_rate_vs_population: f64,
    pub converged_rate: f64,
    pub refill_request_share: f64,
    pub refill_converged_share: f64,
    pub refill_survivor_share: f64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JanusGuardrailReport {
    pub evaluator: &'static str,
    pub search_family: &'static str,
    pub operator_posture: &'static str,
    pub scientific_risk: &'static str,
    pub janus_mode: String,
    pub summary: JanusGuardrailSummary,
    pub generations: Vec<JanusGuardrailGenerationReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JanusGuardrailSummary {
    pub average_duplicate_rate_vs_requests: f64,
    pub average_repopulation_rate_vs_population: f64,
    pub average_converged_rate: f64,
    pub average_refill_request_share: f64,
    pub average_refill_converged_share: f64,
    pub average_refill_survivor_share: f64,
    pub warnings: Vec<String>,
}

pub fn build_janus_guardrail_report(
    janus_mode: &str,
    generations: &[JanusGuardrailGenerationInput<'_>],
    origin_metrics: &[JanusOriginMetricInput<'_>],
) -> JanusGuardrailReport {
    let profile = SearchOperationProfile::for_evaluator(
        EvaluatorFamily::JanusMace,
        SearchFamily::GeneticAlgorithm,
    );

    let generation_reports: Vec<_> = generations
        .iter()
        .map(|generation| build_generation_report(generation, origin_metrics))
        .collect();

    let average_duplicate_rate_vs_requests = average(
        generation_reports
            .iter()
            .map(|row| row.duplicate_rate_vs_requests),
    );
    let average_repopulation_rate_vs_population = average(
        generation_reports
            .iter()
            .map(|row| row.repopulation_rate_vs_population),
    );
    let average_converged_rate = average(generation_reports.iter().map(|row| row.converged_rate));
    let average_refill_request_share = average(
        generation_reports
            .iter()
            .map(|row| row.refill_request_share),
    );
    let average_refill_converged_share = average(
        generation_reports
            .iter()
            .map(|row| row.refill_converged_share),
    );
    let average_refill_survivor_share = average(
        generation_reports
            .iter()
            .map(|row| row.refill_survivor_share),
    );

    let mut warnings = Vec::new();
    if average_duplicate_rate_vs_requests > 0.35 {
        warnings.push(
            "high relaxed duplicate pressure across generations; Janus/MACE local optimization may be collapsing distinct proposals into the same basin".to_string(),
        );
    }
    if average_repopulation_rate_vs_population > 0.25 {
        warnings.push(
            "high repopulation pressure across generations; refill behavior is still carrying too much scientific load for parity claims".to_string(),
        );
    }
    if average_refill_request_share > 0.20 {
        warnings.push(
            "repopulation-origin requests are a large share of total work; initialization/refill generation needs evaluator-aware tuning".to_string(),
        );
    }
    if average_refill_request_share > 0.0 && average_refill_converged_share < 0.70 {
        warnings.push(
            "repopulation-origin convergence is weak; refill operators are producing too many hard-to-relax structures for a stable Janus/MACE GA".to_string(),
        );
    }
    if janus_mode == "local-opt" && average_converged_rate < 0.80 {
        warnings.push(
            "low converged rate under local optimization; force/status mapping and move amplitudes should be reviewed before stronger parity claims".to_string(),
        );
    }

    JanusGuardrailReport {
        evaluator: profile.evaluator.as_str(),
        search_family: profile.family.as_str(),
        operator_posture: profile.operator_posture,
        scientific_risk: profile.scientific_risk,
        janus_mode: janus_mode.to_string(),
        summary: JanusGuardrailSummary {
            average_duplicate_rate_vs_requests,
            average_repopulation_rate_vs_population,
            average_converged_rate,
            average_refill_request_share,
            average_refill_converged_share,
            average_refill_survivor_share,
            warnings,
        },
        generations: generation_reports,
    }
}

fn build_generation_report(
    generation: &JanusGuardrailGenerationInput<'_>,
    origin_metrics: &[JanusOriginMetricInput<'_>],
) -> JanusGuardrailGenerationReport {
    let refill_rows: Vec<_> = origin_metrics
        .iter()
        .filter(|row| {
            row.generation == generation.generation
                && row.phase == generation.phase
                && matches!(row.origin, "REPOPM" | "REPOPR")
        })
        .collect();
    let refill_request_count: usize = refill_rows.iter().map(|row| row.request_count).sum();
    let refill_converged_count: usize = refill_rows.iter().map(|row| row.converged_count).sum();
    let refill_survivor_count: usize = refill_rows.iter().map(|row| row.survivor_count).sum();

    let duplicate_rate_vs_requests = ratio(
        generation.duplicate_count as f64,
        generation.request_count as f64,
    );
    let repopulation_rate_vs_population = ratio(
        generation.repopulated_count as f64,
        generation.population_size as f64,
    );
    let converged_rate = ratio(
        generation.converged_count as f64,
        generation.request_count as f64,
    );
    let refill_request_share = ratio(refill_request_count as f64, generation.request_count as f64);
    let refill_converged_share = ratio(refill_converged_count as f64, refill_request_count as f64);
    let refill_survivor_share = ratio(
        refill_survivor_count as f64,
        generation.population_size as f64,
    );

    let mut warnings = Vec::new();
    if generation.generation == 0 && duplicate_rate_vs_requests > 0.30 {
        warnings.push(
            "initial population duplicate pressure is high after relaxation; evaluator-sensitive seeding is still weak".to_string(),
        );
    }
    if repopulation_rate_vs_population > 0.30 {
        warnings.push(
            "repopulation pressure is high in this generation; refill is compensating for population instability".to_string(),
        );
    }
    if refill_request_share > 0.25 {
        warnings.push(
            "repopulation requests are a large share of this generation; Janus/MACE may need gentler or staged diversity operators".to_string(),
        );
    }
    if refill_request_count > 0 && refill_converged_share < 0.70 {
        warnings.push(
            "repopulation-origin convergence is weak in this generation; refill candidates are not relaxing robustly".to_string(),
        );
    }

    JanusGuardrailGenerationReport {
        generation: generation.generation,
        phase: generation.phase.to_string(),
        duplicate_rate_vs_requests,
        repopulation_rate_vs_population,
        converged_rate,
        refill_request_share,
        refill_converged_share,
        refill_survivor_share,
        warnings,
    }
}

fn ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator <= 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let mut total = 0.0;
    let mut count = 0usize;
    for value in values {
        total += value;
        count += 1;
    }
    ratio(total, count as f64)
}

#[cfg(test)]
mod tests {
    use super::{
        build_janus_guardrail_report, JanusGuardrailGenerationInput, JanusOriginMetricInput,
    };

    #[test]
    fn local_opt_report_flags_duplicate_and_refill_pressure() {
        let report = build_janus_guardrail_report(
            "local-opt",
            &[
                JanusGuardrailGenerationInput {
                    generation: 0,
                    phase: "initialize",
                    request_count: 10,
                    converged_count: 7,
                    population_size: 8,
                    duplicate_count: 4,
                    repopulated_count: 3,
                },
                JanusGuardrailGenerationInput {
                    generation: 1,
                    phase: "evolve",
                    request_count: 10,
                    converged_count: 7,
                    population_size: 8,
                    duplicate_count: 4,
                    repopulated_count: 3,
                },
            ],
            &[JanusOriginMetricInput {
                generation: 1,
                phase: "evolve",
                origin: "REPOPR",
                request_count: 3,
                converged_count: 2,
                survivor_count: 2,
            }],
        );

        assert!(!report.summary.warnings.is_empty());
        assert!(report
            .generations
            .iter()
            .any(|row| !row.warnings.is_empty()));
    }
}
