use crate::DriverJanusMode;
use patina_search::ScottGaOperatorConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JanusGaOperatorPolicy {
    pub profile_name: String,
    pub rationale: Vec<String>,
    pub operator_config: JanusGaOperatorConfigView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaOperatorBackendProfile {
    JanusMace,
    Gulp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JanusGaOperatorConfigView {
    pub pop_replacement_ratio: f64,
    pub reinsert_elites_ratio: f64,
    pub max_repop_attempts: usize,
    pub mutation_ratio: f64,
    pub mut_selfcross_ratio: f64,
    pub cross_1d_2d_ratio: f64,
    pub dim_tolerance: f64,
    pub mutate_swap_ratio: f64,
    pub mutate_expand_ratio: f64,
    pub mutate_contract_ratio: f64,
    pub cluster_crossover_fragment_offset_z: f64,
    pub crossover_attempts: usize,
    pub tournament_size_min: usize,
    pub tournament_size_max: usize,
}

impl JanusGaOperatorPolicy {
    pub fn to_search_config(&self) -> ScottGaOperatorConfig {
        ScottGaOperatorConfig {
            pop_replacement_ratio: self.operator_config.pop_replacement_ratio,
            reinsert_elites_ratio: self.operator_config.reinsert_elites_ratio,
            max_repop_attempts: self.operator_config.max_repop_attempts,
            mutation_ratio: self.operator_config.mutation_ratio,
            mut_selfcross_ratio: self.operator_config.mut_selfcross_ratio,
            cross_1d_2d_ratio: self.operator_config.cross_1d_2d_ratio,
            dim_tolerance: self.operator_config.dim_tolerance,
            mutate_swap_ratio: self.operator_config.mutate_swap_ratio,
            mutate_expand_ratio: self.operator_config.mutate_expand_ratio,
            mutate_contract_ratio: self.operator_config.mutate_contract_ratio,
            cluster_crossover_fragment_offset_z: self
                .operator_config
                .cluster_crossover_fragment_offset_z,
            crossover_attempts: self.operator_config.crossover_attempts,
            tournament_size_min: self.operator_config.tournament_size_min,
            tournament_size_max: self.operator_config.tournament_size_max,
        }
    }
}

pub fn select_janus_ga_operator_policy(
    backend_profile: GaOperatorBackendProfile,
    janus_mode: DriverJanusMode,
    step_size: f64,
) -> JanusGaOperatorPolicy {
    let mut config = ScottGaOperatorConfig::default();
    let mut rationale = Vec::new();

    match backend_profile {
        GaOperatorBackendProfile::JanusMace => {
            match janus_mode {
                DriverJanusMode::LocalOpt => {
                    config.pop_replacement_ratio = 0.75;
                    config.reinsert_elites_ratio = 0.35;
                    config.max_repop_attempts = 96;
                    config.mutation_ratio = 0.60;
                    config.mut_selfcross_ratio = 0.10;
                    config.mutate_expand_ratio = 0.10;
                    config.mutate_contract_ratio = 0.10;
                    config.crossover_attempts = 6;
                    rationale.push(
                        "local optimization with Janus/MACE can collapse diversity earlier than GULP, so replacement pressure is reduced and refill search is widened".to_string(),
                    );
                    rationale.push(
                        "mutation and self-crossover are damped to avoid generating proposals that simply relax back into the same basin".to_string(),
                    );
                    rationale.push(
                        "extra crossover attempts are allowed so structurally distinct children have a better chance to survive pre-relaxation filtering".to_string(),
                    );
                }
                DriverJanusMode::SinglePoint => {
                    config.pop_replacement_ratio = 0.85;
                    config.reinsert_elites_ratio = 0.45;
                    config.mutation_ratio = 0.70;
                    config.mut_selfcross_ratio = 0.15;
                    rationale.push(
                        "single-point Janus runs stay closer to reference operator behavior, but still keep slightly softer replacement and mutation than GULP defaults".to_string(),
                    );
                }
            }

            if step_size > 0.35 {
                config.mutation_ratio *= 0.9;
                config.mut_selfcross_ratio *= 0.8;
                rationale.push(
                    "large requested step sizes trigger additional damping so evaluator-side relaxation does not dominate the search with over-aggressive proposals".to_string(),
                );
            }
        }
        GaOperatorBackendProfile::Gulp => {
            config.max_repop_attempts = 96;
            rationale.push(
                "standalone GULP runs keep the reference Scott-style operator defaults rather than Janus-specific diversity damping".to_string(),
            );
            rationale.push(
                "initial 0D population generation uses bounded random packing, so refill attempts are capped to avoid spending startup time in rejection sampling before the first evaluation".to_string(),
            );
        }
    }

    JanusGaOperatorPolicy {
        profile_name: match backend_profile {
            GaOperatorBackendProfile::Gulp => "gulp_reference_bias".to_string(),
            GaOperatorBackendProfile::JanusMace => match janus_mode {
                DriverJanusMode::LocalOpt => "janus_local_opt_diversity_guard".to_string(),
                DriverJanusMode::SinglePoint => "janus_single_point_reference_bias".to_string(),
            },
        },
        rationale,
        operator_config: JanusGaOperatorConfigView {
            pop_replacement_ratio: config.pop_replacement_ratio,
            reinsert_elites_ratio: config.reinsert_elites_ratio,
            max_repop_attempts: config.max_repop_attempts,
            mutation_ratio: config.mutation_ratio,
            mut_selfcross_ratio: config.mut_selfcross_ratio,
            cross_1d_2d_ratio: config.cross_1d_2d_ratio,
            dim_tolerance: config.dim_tolerance,
            mutate_swap_ratio: config.mutate_swap_ratio,
            mutate_expand_ratio: config.mutate_expand_ratio,
            mutate_contract_ratio: config.mutate_contract_ratio,
            cluster_crossover_fragment_offset_z: config.cluster_crossover_fragment_offset_z,
            crossover_attempts: config.crossover_attempts,
            tournament_size_min: config.tournament_size_min,
            tournament_size_max: config.tournament_size_max,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{select_janus_ga_operator_policy, GaOperatorBackendProfile};
    use crate::DriverJanusMode;
    use patina_search::ScottGaOperatorConfig;

    #[test]
    fn local_opt_policy_is_more_conservative_than_default() {
        let policy = select_janus_ga_operator_policy(
            GaOperatorBackendProfile::JanusMace,
            DriverJanusMode::LocalOpt,
            0.1,
        );
        assert_eq!(policy.profile_name, "janus_local_opt_diversity_guard");
        assert!(policy.operator_config.pop_replacement_ratio < 0.95);
        assert!(policy.operator_config.mutation_ratio < 0.8);
    }

    #[test]
    fn large_step_size_damps_mutation() {
        let small = select_janus_ga_operator_policy(
            GaOperatorBackendProfile::JanusMace,
            DriverJanusMode::LocalOpt,
            0.1,
        );
        let large = select_janus_ga_operator_policy(
            GaOperatorBackendProfile::JanusMace,
            DriverJanusMode::LocalOpt,
            0.5,
        );
        assert!(large.operator_config.mutation_ratio < small.operator_config.mutation_ratio);
        assert!(
            large.operator_config.mut_selfcross_ratio < small.operator_config.mut_selfcross_ratio
        );
    }

    #[test]
    fn gulp_policy_reports_reference_profile() {
        let policy = select_janus_ga_operator_policy(
            GaOperatorBackendProfile::Gulp,
            DriverJanusMode::LocalOpt,
            0.5,
        );
        assert_eq!(policy.profile_name, "gulp_reference_bias");
        assert_eq!(
            policy.operator_config.pop_replacement_ratio,
            ScottGaOperatorConfig::default().pop_replacement_ratio
        );
        assert_eq!(
            policy.operator_config.mutation_ratio,
            ScottGaOperatorConfig::default().mutation_ratio
        );
    }
}
