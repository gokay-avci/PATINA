use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Native `klmc_scott` entry points preserved as black-box scientific algorithms.
///
/// Rust should treat these as dispatcher labels into the legacy Fortran kernel
/// rather than as invitations to reimplement the underlying science.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyScottJobType {
    ProductionRun,
    BasinHopping,
    GeneticAlgorithm,
    SolidSolutions,
    RefineSprings,
    ScanBox,
    SimulatedAnnealing,
    EnergyLid,
    Testing,
    HybridGaProduction,
}

impl LegacyScottJobType {
    pub const ALL: [Self; 10] = [
        Self::ProductionRun,
        Self::BasinHopping,
        Self::GeneticAlgorithm,
        Self::SolidSolutions,
        Self::RefineSprings,
        Self::ScanBox,
        Self::SimulatedAnnealing,
        Self::EnergyLid,
        Self::Testing,
        Self::HybridGaProduction,
    ];

    pub const fn job_code(self) -> u8 {
        match self {
            Self::ProductionRun => 0,
            Self::BasinHopping => 1,
            Self::GeneticAlgorithm => 2,
            Self::SolidSolutions => 3,
            Self::RefineSprings => 4,
            Self::ScanBox => 5,
            Self::SimulatedAnnealing => 6,
            Self::EnergyLid => 7,
            Self::Testing => 8,
            Self::HybridGaProduction => 9,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProductionRun => "production_run",
            Self::BasinHopping => "basin_hopping",
            Self::GeneticAlgorithm => "genetic_algorithm",
            Self::SolidSolutions => "solid_solutions",
            Self::RefineSprings => "refine_springs",
            Self::ScanBox => "scan_box",
            Self::SimulatedAnnealing => "simulated_annealing",
            Self::EnergyLid => "energy_lid",
            Self::Testing => "testing",
            Self::HybridGaProduction => "hybrid_ga_production",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct LegacyScottExecutionRequest {
    pub job_type: LegacyScottJobType,
    pub scott_bin: PathBuf,
    pub data_dir: PathBuf,
    pub workdir: PathBuf,
    pub run_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyScottCheckpoint {
    pub name: &'static str,
    pub objective: &'static str,
    pub principles: &'static [&'static str],
    pub checklist: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyScottSupportTier {
    UncharacterizedNativeLaunch,
    GenericExportCharacterized,
    TypedInterpretationCharacterized,
}

impl LegacyScottSupportTier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UncharacterizedNativeLaunch => "uncharacterized_native_launch",
            Self::GenericExportCharacterized => "generic_export_characterized",
            Self::TypedInterpretationCharacterized => "typed_interpretation_characterized",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LegacyScottFixtureRequirement {
    pub path_hint: &'static str,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LegacyScottCharacterization {
    pub job_type: LegacyScottJobType,
    pub support_tier: LegacyScottSupportTier,
    pub fixture_requirements: &'static [LegacyScottFixtureRequirement],
    pub note: &'static str,
}

#[allow(dead_code)]
pub trait LegacyScottExecutionPort {
    type Summary;

    fn execute(&self, request: &LegacyScottExecutionRequest) -> Result<Self::Summary>;
}

const LEGACY_SCOTT_PRINCIPLES: &[&str] = &[
    "Treat `klmc_scott` as a preserved black-box scientific kernel.",
    "Replace `klmc3_taskfarm` with patina-side orchestration and sharding only.",
    "Keep native `run.job`, `Master.gin`, `jobs`, `atoms.in`, and output-tree contracts intact.",
    "Separate generic native launch from optional mode-specific artifact interpretation.",
    "Expand CLI access to native entry points without claiming Rust ownership of their algorithms.",
];

const LEGACY_SCOTT_CHECKLIST: &[&str] = &[
    "Add a native-only `LegacyScottJobType` vocabulary in the driver application layer.",
    "Keep Rust-owned monolithic and persistent-daemon paths intact and explicitly separate.",
    "Promote generic sandbox staging, launch, timeout, and raw artifact capture into a native adapter boundary.",
    "Restrict BH/GA typed trace recovery to the modes already characterized; preserve raw outputs for the rest first.",
    "Characterize each native `JOB_TYPE` with a real fixture deck before adding richer Rust-side interpretation.",
];

const NO_FIXTURE_REQUIREMENTS: &[LegacyScottFixtureRequirement] = &[];

const TESTING_FIXTURE_REQUIREMENTS: &[LegacyScottFixtureRequirement] = &[LegacyScottFixtureRequirement {
    path_hint: "A11.xyz",
    reason: "Testing mode reads `A11.xyz` from the working directory before running graph/hashkey checks.",
}];

const LEGACY_SCOTT_CHARACTERIZATION_MATRIX: &[LegacyScottCharacterization] = &[
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::ProductionRun,
        support_tier: LegacyScottSupportTier::UncharacterizedNativeLaunch,
        fixture_requirements: NO_FIXTURE_REQUIREMENTS,
        note: "Native dispatcher is exposed, but this entry point still needs a characterized fixture bundle.",
    },
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::BasinHopping,
        support_tier: LegacyScottSupportTier::TypedInterpretationCharacterized,
        fixture_requirements: NO_FIXTURE_REQUIREMENTS,
        note: "Native launch plus BH-specific Rust trace recovery is already wired through the preserved export bridge.",
    },
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::GeneticAlgorithm,
        support_tier: LegacyScottSupportTier::TypedInterpretationCharacterized,
        fixture_requirements: NO_FIXTURE_REQUIREMENTS,
        note: "Native launch plus GA-specific Rust trace recovery is already wired through the preserved export bridge.",
    },
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::SolidSolutions,
        support_tier: LegacyScottSupportTier::GenericExportCharacterized,
        fixture_requirements: NO_FIXTURE_REQUIREMENTS,
        note: "Generic native export was proven with the legacy solid-solution example, but typed interpretation is intentionally absent.",
    },
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::RefineSprings,
        support_tier: LegacyScottSupportTier::UncharacterizedNativeLaunch,
        fixture_requirements: NO_FIXTURE_REQUIREMENTS,
        note: "Native dispatcher is exposed, but this entry point still needs a characterized fixture bundle.",
    },
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::ScanBox,
        support_tier: LegacyScottSupportTier::UncharacterizedNativeLaunch,
        fixture_requirements: NO_FIXTURE_REQUIREMENTS,
        note: "Native dispatcher is exposed, but this entry point still needs a characterized fixture bundle.",
    },
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::SimulatedAnnealing,
        support_tier: LegacyScottSupportTier::UncharacterizedNativeLaunch,
        fixture_requirements: NO_FIXTURE_REQUIREMENTS,
        note: "Native dispatcher is exposed, but this entry point still needs a characterized fixture bundle.",
    },
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::EnergyLid,
        support_tier: LegacyScottSupportTier::UncharacterizedNativeLaunch,
        fixture_requirements: NO_FIXTURE_REQUIREMENTS,
        note: "Native dispatcher is exposed, but this entry point still needs a characterized fixture bundle.",
    },
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::Testing,
        support_tier: LegacyScottSupportTier::GenericExportCharacterized,
        fixture_requirements: TESTING_FIXTURE_REQUIREMENTS,
        note: "Generic native export was proven with the testing entry point after preseeding the legacy workspace fixture.",
    },
    LegacyScottCharacterization {
        job_type: LegacyScottJobType::HybridGaProduction,
        support_tier: LegacyScottSupportTier::UncharacterizedNativeLaunch,
        fixture_requirements: NO_FIXTURE_REQUIREMENTS,
        note: "Native dispatcher is exposed, but this entry point still needs a characterized fixture bundle.",
    },
];

pub const LEGACY_SCOTT_CHECKPOINT: LegacyScottCheckpoint = LegacyScottCheckpoint {
    name: "legacy_scott_blackbox_strangler",
    objective: "Preserve native KLMC3 algorithms as an optional hexagonal adapter in PATINA.",
    principles: LEGACY_SCOTT_PRINCIPLES,
    checklist: LEGACY_SCOTT_CHECKLIST,
};

impl LegacyScottJobType {
    pub fn characterization(self) -> &'static LegacyScottCharacterization {
        legacy_scott_characterization_matrix()
            .iter()
            .find(|characterization| characterization.job_type == self)
            .expect("every legacy Scott job type must be characterized in the support matrix")
    }
}

pub fn legacy_scott_characterization_matrix() -> &'static [LegacyScottCharacterization] {
    LEGACY_SCOTT_CHARACTERIZATION_MATRIX
}

pub fn render_legacy_scott_checkpoint() -> String {
    let supported_jobs = LegacyScottJobType::ALL
        .iter()
        .map(|job| format!("{}={}", job.job_code(), job.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let principles = LEGACY_SCOTT_CHECKPOINT
        .principles
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    let checklist = LEGACY_SCOTT_CHECKPOINT
        .checklist
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    let characterization = LegacyScottJobType::ALL
        .iter()
        .map(|job| {
            let entry = job.characterization();
            let fixture_suffix = if entry.fixture_requirements.is_empty() {
                String::new()
            } else {
                let fixtures = entry
                    .fixture_requirements
                    .iter()
                    .map(|fixture| format!("{} ({})", fixture.path_hint, fixture.reason))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("; fixtures: {fixtures}")
            };
            format!(
                "- {}={}: {}{}",
                entry.job_type.job_code(),
                entry.job_type.as_str(),
                entry.support_tier.as_str(),
                fixture_suffix,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Legacy SCOTT checkpoint: {name}\nObjective: {objective}\nNative entry points: {supported_jobs}\nPrinciples:\n{principles}\nChecklist:\n{checklist}\nCharacterization:\n{characterization}",
        name = LEGACY_SCOTT_CHECKPOINT.name,
        objective = LEGACY_SCOTT_CHECKPOINT.objective,
        supported_jobs = supported_jobs,
        principles = principles,
        checklist = checklist,
        characterization = characterization,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        legacy_scott_characterization_matrix, render_legacy_scott_checkpoint, LegacyScottJobType,
        LegacyScottSupportTier, LEGACY_SCOTT_CHECKPOINT,
    };

    #[test]
    fn native_job_type_codes_match_legacy_scott_dispatch() {
        assert_eq!(LegacyScottJobType::ProductionRun.job_code(), 0);
        assert_eq!(LegacyScottJobType::BasinHopping.job_code(), 1);
        assert_eq!(LegacyScottJobType::GeneticAlgorithm.job_code(), 2);
        assert_eq!(LegacyScottJobType::HybridGaProduction.job_code(), 9);
    }

    #[test]
    fn checkpoint_renderer_mentions_blackbox_intent() {
        let checkpoint = render_legacy_scott_checkpoint();
        assert!(checkpoint.contains(LEGACY_SCOTT_CHECKPOINT.name));
        assert!(checkpoint.contains("black-box scientific kernel"));
        assert!(checkpoint.contains("LegacyScottJobType"));
    }

    #[test]
    fn characterization_matrix_covers_all_native_job_types() {
        let mut jobs = legacy_scott_characterization_matrix()
            .iter()
            .map(|entry| entry.job_type)
            .collect::<Vec<_>>();
        jobs.sort_by_key(|job| job.job_code());
        assert_eq!(jobs, LegacyScottJobType::ALL);
    }

    #[test]
    fn testing_mode_records_workspace_fixture_requirement() {
        let testing = LegacyScottJobType::Testing.characterization();
        assert_eq!(
            testing.support_tier,
            LegacyScottSupportTier::GenericExportCharacterized
        );
        assert_eq!(testing.fixture_requirements.len(), 1);
        assert_eq!(testing.fixture_requirements[0].path_hint, "A11.xyz");
    }
}
