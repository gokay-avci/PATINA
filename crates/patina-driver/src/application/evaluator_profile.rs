use super::policy::SearchFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluatorFamily {
    GulpLikeReference,
    JanusMace,
}

impl EvaluatorFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GulpLikeReference => "gulp_like_reference",
            Self::JanusMace => "janus_mace",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationAdaptationLevel {
    NativeReference,
    ConservativeAdjustment,
    StrongAdjustment,
}

impl OperationAdaptationLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeReference => "native_reference",
            Self::ConservativeAdjustment => "conservative_adjustment",
            Self::StrongAdjustment => "strong_adjustment",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOperationProfile {
    pub evaluator: EvaluatorFamily,
    pub family: SearchFamily,
    pub adaptation_level: OperationAdaptationLevel,
    pub operator_posture: &'static str,
    pub scientific_risk: &'static str,
}

impl SearchOperationProfile {
    pub fn for_evaluator(
        evaluator: EvaluatorFamily,
        family: SearchFamily,
    ) -> SearchOperationProfile {
        match (evaluator, family) {
            (EvaluatorFamily::GulpLikeReference, SearchFamily::ProductionRun) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::NativeReference,
                operator_posture: "treat native production intake, evaluator progression, rejection rules, and best-set handling as the scientific reference",
                scientific_risk: "low, provided restart and artifact semantics remain native-aligned",
            },
            (EvaluatorFamily::GulpLikeReference, SearchFamily::GeneticAlgorithm) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::NativeReference,
                operator_posture: "treat SCOTT/GULP behavior as the scientific reference for selection, crossover, mutation, replacement, and duplicate flow",
                scientific_risk: "low, provided native artifacts remain the parity baseline",
            },
            (EvaluatorFamily::GulpLikeReference, SearchFamily::BasinHopping) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::NativeReference,
                operator_posture: "keep native BH move classes, step adaptation, and acceptance semantics intact",
                scientific_risk: "low, provided BH remains native-owned",
            },
            (EvaluatorFamily::GulpLikeReference, SearchFamily::SolidSolutions) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::NativeReference,
                operator_posture: "treat native solution mixing, vacancy handling, and region-aware bookkeeping as the reference contract",
                scientific_risk: "medium because workflow coverage on the Rust side is still narrow",
            },
            (EvaluatorFamily::GulpLikeReference, SearchFamily::ScanSurface) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::NativeReference,
                operator_posture: "preserve supported-cluster scan semantics and substrate-relative movement rules from native SCOTT",
                scientific_risk: "medium until surface workflow fixtures are preserved in Rust",
            },
            (EvaluatorFamily::GulpLikeReference, SearchFamily::SimulatedAnnealing) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::NativeReference,
                operator_posture: "preserve native annealing temperature, acceptance, and restart semantics before introducing async or research variants",
                scientific_risk: "medium until annealing workflow artifacts are parity-tested",
            },
            (EvaluatorFamily::GulpLikeReference, SearchFamily::EnergyLid) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::NativeReference,
                operator_posture: "treat threshold windows, runner transitions, and basin reporting as native-owned semantics",
                scientific_risk: "medium until energy-lid traces and thresholds are fixture-backed in Rust",
            },
            (EvaluatorFamily::GulpLikeReference, SearchFamily::HybridGaProduction) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::NativeReference,
                operator_posture: "preserve the combined GA-production control meaning before allowing Rust-specific workflow reshaping",
                scientific_risk: "medium because the hybrid lane is scientifically richer than either isolated controller shell",
            },
            (EvaluatorFamily::JanusMace, SearchFamily::ProductionRun) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::ConservativeAdjustment,
                operator_posture: "preserve native production semantics while adapting status mapping, retry budgets, and failure normalization to Janus/MACE behavior",
                scientific_risk: "medium because evaluator status mapping can distort rejection and best-set updates if treated casually",
            },
            (EvaluatorFamily::JanusMace, SearchFamily::GeneticAlgorithm) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::StrongAdjustment,
                operator_posture: "preserve SCOTT lifecycle shape, but treat initialization, refill, mutation amplitude, and duplicate pressure as evaluator-sensitive because local relaxation can collapse diversity earlier than GULP",
                scientific_risk: "high if GULP-era operator aggressiveness is copied without parity checks on relaxed-population diversity",
            },
            (EvaluatorFamily::JanusMace, SearchFamily::BasinHopping) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::StrongAdjustment,
                operator_posture: "do not claim BH parity from generic Rust perturb-and-accept loops; BH against Janus/MACE needs native evidence or an explicit fork with move, acceptance, and force-driven status mapping adapted deliberately",
                scientific_risk: "high because evaluator force/relaxation behavior changes basin identity and acceptance statistics",
            },
            (EvaluatorFamily::JanusMace, SearchFamily::SolidSolutions) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::StrongAdjustment,
                operator_posture: "keep solution semantics Scott-shaped, but expect strong evaluator-sensitive adjustment before any parity claim under Janus/MACE",
                scientific_risk: "high because workflow meaning mixes compositional rules with evaluator-dependent relaxation behavior",
            },
            (EvaluatorFamily::JanusMace, SearchFamily::ScanSurface) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::StrongAdjustment,
                operator_posture: "treat supported-cluster scan workflows as parity work first; substrate-relative motion and evaluator-induced basin reshaping need preserved fixtures",
                scientific_risk: "high until surface-native evidence exists",
            },
            (EvaluatorFamily::JanusMace, SearchFamily::SimulatedAnnealing) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::StrongAdjustment,
                operator_posture: "do not let async or learning-oriented annealing variants masquerade as Scott parity under Janus/MACE without fixture-backed controller evidence",
                scientific_risk: "high because annealing dynamics are sensitive to evaluator-induced energy and force differences",
            },
            (EvaluatorFamily::JanusMace, SearchFamily::EnergyLid) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::StrongAdjustment,
                operator_posture: "preserve threshold and basin-transition semantics, but assume Janus/MACE needs explicit evaluator-aware thresholds and evidence before parity claims",
                scientific_risk: "high because lid transitions and basin identity are evaluator-sensitive",
            },
            (EvaluatorFamily::JanusMace, SearchFamily::HybridGaProduction) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::StrongAdjustment,
                operator_posture: "treat hybrid workflows as explicit parity programs, not as a casual merge of existing Janus-friendly controller pieces",
                scientific_risk: "high until both production and GA semantics are stable under the same evaluator contract",
            },
            (EvaluatorFamily::JanusMace, SearchFamily::FutureScottFork) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::StrongAdjustment,
                operator_posture: "start from evaluator-aware policies and parity fixtures rather than inheriting SCOTT/GULP operator defaults",
                scientific_risk: "high until the new fork defines its own validated operator contract",
            },
            (EvaluatorFamily::GulpLikeReference, SearchFamily::FutureScottFork) => Self {
                evaluator,
                family,
                adaptation_level: OperationAdaptationLevel::ConservativeAdjustment,
                operator_posture: "treat any new Rust fork as an explicit research path even when GULP-like energetics remain available",
                scientific_risk: "medium until fixed-seed differential comparisons are in place",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EvaluatorFamily, OperationAdaptationLevel, SearchOperationProfile};
    use crate::application::policy::SearchFamily;

    #[test]
    fn janus_ga_requires_strong_adjustment() {
        let profile = SearchOperationProfile::for_evaluator(
            EvaluatorFamily::JanusMace,
            SearchFamily::GeneticAlgorithm,
        );
        assert_eq!(
            profile.adaptation_level,
            OperationAdaptationLevel::StrongAdjustment
        );
    }

    #[test]
    fn gulp_bh_is_reference_owned() {
        let profile = SearchOperationProfile::for_evaluator(
            EvaluatorFamily::GulpLikeReference,
            SearchFamily::BasinHopping,
        );
        assert_eq!(
            profile.adaptation_level,
            OperationAdaptationLevel::NativeReference
        );
    }

    #[test]
    fn janus_production_is_conservative_adjustment() {
        let profile = SearchOperationProfile::for_evaluator(
            EvaluatorFamily::JanusMace,
            SearchFamily::ProductionRun,
        );
        assert_eq!(
            profile.adaptation_level,
            OperationAdaptationLevel::ConservativeAdjustment
        );
    }
}
