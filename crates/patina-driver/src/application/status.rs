use super::evaluator_profile::{EvaluatorFamily, SearchOperationProfile};
use super::legacy_scott::render_legacy_scott_checkpoint;
use super::policy::{SearchFamily, SearchForkPolicy, WorkflowLane, PORT_ARCHITECTURE_RULES};

pub fn render_status_report() -> String {
    let production_policy = SearchForkPolicy::for_family(SearchFamily::ProductionRun);
    let ga_policy = SearchForkPolicy::for_family(SearchFamily::GeneticAlgorithm);
    let bh_policy = SearchForkPolicy::for_family(SearchFamily::BasinHopping);
    let energy_lid_policy = SearchForkPolicy::for_family(SearchFamily::EnergyLid);
    let annealing_policy = SearchForkPolicy::for_family(SearchFamily::SimulatedAnnealing);
    let workflow_taxonomy = SearchForkPolicy::current()
        .into_iter()
        .map(|policy| format!("{}:{}", policy.family.as_str(), policy.readiness.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let janus_ga_profile = SearchOperationProfile::for_evaluator(
        EvaluatorFamily::JanusMace,
        SearchFamily::GeneticAlgorithm,
    );
    let janus_bh_profile = SearchOperationProfile::for_evaluator(
        EvaluatorFamily::JanusMace,
        SearchFamily::BasinHopping,
    );
    let janus_production_profile = SearchOperationProfile::for_evaluator(
        EvaluatorFamily::JanusMace,
        SearchFamily::ProductionRun,
    );
    let gulp_ga_profile = SearchOperationProfile::for_evaluator(
        EvaluatorFamily::GulpLikeReference,
        SearchFamily::GeneticAlgorithm,
    );

    format!(
        "Native workflow: SCOTT owns production, GA, BH, energy-lid, annealing, and the broader workflow family semantics while Rust patches templates, stages files, and tracks runs.\n\
Worker workflow: Rust owns evaluator scheduling for transient workers generically, but persistent parallel workers are a daemon-style contract in the new Rust-owned branch.\n\
Persistent daemon: implemented for Rust worker campaigns through the Janus/MACE backend.\n\
Persistent SCOTT worker: not implemented yet.\n\
Current rule: use `{}` for native scientific searches, `{}` for Scott-shaped Rust parity workflows, and `{}` for the Rust-owned persistent-daemon branch.\n\
Scientific duplicate default: the Rust Janus lane uses `external-native-hashkey` with direct `patina-dreadnaut` canonical identity filtering and built-in species radii; `atoms.in` is now only an optional override.\n\
Filter taxonomy: exact identity filters, threshold duplicate filters, and continuous descriptors are treated as separate scientific categories so future Pertuber-style descriptors do not get conflated with SCOTT duplicate identity.\n\
Production fork policy: `{}` is in `{}` mode; native scope is `{}` and Rust scope is `{}` because production is the first full Scott workflow targeted for parity.\n\
GA fork policy: `{}` is in `{}` mode; native scope is `{}` and Rust scope is `{}` until fixed-seed differential evidence is strong.\n\
BH fork policy: `{}` remains `{}`; native scope is `{}` and Rust scope is `{}` so Rust should export and normalize native BH state instead of casually reimplementing it.\n\
Energy-lid fork policy: `{}` is currently `{}`; native scope is `{}` and Rust scope is `{}` while the workflow family expands beyond GA/BH.\n\
Annealing fork policy: `{}` is currently `{}`; native scope is `{}` and Rust scope is `{}` while parity planning is still ahead of controller ownership.\n\
Workflow taxonomy: {}.\n\
Reference evaluator posture: `{}` keeps `{}`.\n\
Janus/MACE production guardrail: `{}` with `{}`.\n\
Janus/MACE GA guardrail: `{}` with `{}`.\n\
Janus/MACE BH guardrail: `{}` with `{}`.\n\
Hexagonal rule: {}.\n\
{}",
        WorkflowLane::NativeScott.as_str(),
        WorkflowLane::RustScottParity.as_str(),
        WorkflowLane::RustOwnedJanus.as_str(),
        production_policy.family.as_str(),
        production_policy.readiness.as_str(),
        production_policy.native_lane_scope.as_str(),
        production_policy.rust_lane_scope.as_str(),
        ga_policy.family.as_str(),
        ga_policy.readiness.as_str(),
        ga_policy.native_lane_scope.as_str(),
        ga_policy.rust_lane_scope.as_str(),
        bh_policy.family.as_str(),
        bh_policy.readiness.as_str(),
        bh_policy.native_lane_scope.as_str(),
        bh_policy.rust_lane_scope.as_str(),
        energy_lid_policy.family.as_str(),
        energy_lid_policy.readiness.as_str(),
        energy_lid_policy.native_lane_scope.as_str(),
        energy_lid_policy.rust_lane_scope.as_str(),
        annealing_policy.family.as_str(),
        annealing_policy.readiness.as_str(),
        annealing_policy.native_lane_scope.as_str(),
        annealing_policy.rust_lane_scope.as_str(),
        workflow_taxonomy,
        gulp_ga_profile.evaluator.as_str(),
        gulp_ga_profile.operator_posture,
        janus_production_profile.adaptation_level.as_str(),
        janus_production_profile.operator_posture,
        janus_ga_profile.adaptation_level.as_str(),
        janus_ga_profile.operator_posture,
        janus_bh_profile.adaptation_level.as_str(),
        janus_bh_profile.operator_posture,
        PORT_ARCHITECTURE_RULES.join("; "),
        render_legacy_scott_checkpoint(),
    )
}

#[cfg(test)]
mod tests {
    use super::render_status_report;

    #[test]
    fn status_report_mentions_rust_and_native_lanes() {
        let report = render_status_report();
        assert!(report.contains("native_scott_search"));
        assert!(report.contains("rust_scott_parity"));
        assert!(report.contains("rust_persistent_daemon"));
    }

    #[test]
    fn status_report_mentions_bh_native_only_policy() {
        let report = render_status_report();
        assert!(report.contains("basin_hopping"));
        assert!(report.contains("native_only"));
    }

    #[test]
    fn status_report_mentions_broader_workflow_taxonomy() {
        let report = render_status_report();
        assert!(report.contains("production_run"));
        assert!(report.contains("scan_surface"));
        assert!(report.contains("solid_solutions"));
        assert!(report.contains("energy_lid"));
        assert!(report.contains("simulated_annealing"));
    }

    #[test]
    fn status_report_mentions_legacy_scott_checkpoint() {
        let report = render_status_report();
        assert!(report.contains("Legacy SCOTT checkpoint"));
        assert!(report.contains("legacy_scott_blackbox_strangler"));
    }
}
