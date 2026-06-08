use serde::{Deserialize, Serialize};

/// One-based Scott stage index matching native `edefn` semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StageIndex(pub u8);

impl StageIndex {
    pub const FIRST: Self = Self(1);

    pub fn get(self) -> u8 {
        self.0
    }
}

/// Native Scott energy-landscape code for a procedure stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageEngine {
    Gulp,
    Cp2k,
    Crystal,
    Aims,
    Vasp,
    Nwchem,
    Dmol,
    NoEvalExport,
}

impl StageEngine {
    /// Parses the native one-letter Scott `DEF_ENERGY` code.
    pub fn from_native_code(code: char) -> Option<Self> {
        match code.to_ascii_uppercase() {
            'G' => Some(Self::Gulp),
            'C' => Some(Self::Cp2k),
            'R' => Some(Self::Crystal),
            'A' => Some(Self::Aims),
            'V' => Some(Self::Vasp),
            'N' => Some(Self::Nwchem),
            'D' => Some(Self::Dmol),
            'X' => Some(Self::NoEvalExport),
            _ => None,
        }
    }
}

/// Native Scott policy for what to do if the final refinement stage fails.
///
/// In Fortran this is controlled by `L_ONLY_2ND_ENERGY`, which is phrased to users as:
/// "reject LM if final refinement fails" vs "keep LM even if final refinement fails".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalStageFailurePolicy {
    KeepPreviousAccepted,
    RejectCandidate,
}

impl FinalStageFailurePolicy {
    /// Maps the native Scott `ONLY_2ND_ENERGY` flag into the explicit Rust policy.
    pub fn from_only_2nd_energy(only_2nd_energy: bool) -> Self {
        if only_2nd_energy {
            Self::RejectCandidate
        } else {
            Self::KeepPreviousAccepted
        }
    }
}

/// What to do after a stage completes successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageTransition {
    Stop,
    Continue,
}

/// A single stage in the Scott evaluator procedure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StagePlan {
    pub stage: StageIndex,
    pub engine: StageEngine,
    pub refine_if_energy_below: Option<f64>,
    pub energy_min_threshold: Option<f64>,
    pub energy_max_threshold: Option<f64>,
    pub keep_only_if_final_stage: bool,
}

/// Ordered stage selection for the current evaluation procedure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StageSelection {
    pub stages: Vec<StagePlan>,
}

impl StageSelection {
    /// Builds an ordered stage selection from native Scott array-shaped inputs.
    pub fn from_native_scott(
        engine_codes: &[char],
        refine_thresholds: &[Option<f64>],
        energy_min_thresholds: &[Option<f64>],
        energy_max_thresholds: &[Option<f64>],
        keep_only_if_final_stage: bool,
    ) -> Result<Self, StageSelectionError> {
        if engine_codes.is_empty() {
            return Err(StageSelectionError::Empty);
        }
        if refine_thresholds.len() != engine_codes.len()
            || energy_min_thresholds.len() != engine_codes.len()
            || energy_max_thresholds.len() != engine_codes.len()
        {
            return Err(StageSelectionError::MismatchedNativeInputLengths {
                engines: engine_codes.len(),
                refine_thresholds: refine_thresholds.len(),
                energy_min_thresholds: energy_min_thresholds.len(),
                energy_max_thresholds: energy_max_thresholds.len(),
            });
        }

        let mut stages = Vec::with_capacity(engine_codes.len());
        for (idx, code) in engine_codes.iter().copied().enumerate() {
            let Some(engine) = StageEngine::from_native_code(code) else {
                return Err(StageSelectionError::UnknownEngineCode(code));
            };
            stages.push(StagePlan {
                stage: StageIndex(idx as u8 + 1),
                engine,
                refine_if_energy_below: refine_thresholds[idx],
                energy_min_threshold: energy_min_thresholds[idx],
                energy_max_threshold: energy_max_thresholds[idx],
                keep_only_if_final_stage,
            });
        }

        let selection = Self { stages };
        selection.validate()?;
        Ok(selection)
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.stages.len()
    }

    pub fn first(&self) -> Option<&StagePlan> {
        self.stages.first()
    }

    pub fn get(&self, stage: StageIndex) -> Option<&StagePlan> {
        self.stages.iter().find(|entry| entry.stage == stage)
    }

    pub fn next_after(&self, stage: StageIndex) -> Option<&StagePlan> {
        self.stages
            .iter()
            .find(|entry| entry.stage.get() == stage.get().saturating_add(1))
    }

    pub fn validate(&self) -> Result<(), StageSelectionError> {
        if self.stages.is_empty() {
            return Err(StageSelectionError::Empty);
        }

        for (offset, stage) in self.stages.iter().enumerate() {
            let expected = offset as u8 + 1;
            if stage.stage.get() != expected {
                return Err(StageSelectionError::NonContiguousStages {
                    expected,
                    found: stage.stage.get(),
                });
            }
        }

        Ok(())
    }

    pub fn transition_for_energy(
        &self,
        stage: StageIndex,
        accepted_energy: f64,
    ) -> Result<StageTransition, StageSelectionError> {
        let current = self
            .get(stage)
            .ok_or(StageSelectionError::UnknownStage(stage.get()))?;
        let Some(next_stage) = self.next_after(stage) else {
            return Ok(StageTransition::Stop);
        };

        match current.refine_if_energy_below {
            Some(threshold) if accepted_energy < threshold => Ok(StageTransition::Continue),
            Some(_) => Ok(StageTransition::Stop),
            None => {
                let _ = next_stage;
                Ok(StageTransition::Continue)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageSelectionError {
    Empty,
    NonContiguousStages {
        expected: u8,
        found: u8,
    },
    UnknownStage(u8),
    UnknownEngineCode(char),
    MismatchedNativeInputLengths {
        engines: usize,
        refine_thresholds: usize,
        energy_min_thresholds: usize,
        energy_max_thresholds: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        FinalStageFailurePolicy, StageEngine, StageIndex, StagePlan, StageSelection,
        StageTransition,
    };

    fn make_stage(stage: u8, threshold: Option<f64>) -> StagePlan {
        StagePlan {
            stage: StageIndex(stage),
            engine: StageEngine::Gulp,
            refine_if_energy_below: threshold,
            energy_min_threshold: None,
            energy_max_threshold: None,
            keep_only_if_final_stage: false,
        }
    }

    #[test]
    fn stage_selection_requires_contiguous_one_based_indices() {
        let selection = StageSelection {
            stages: vec![make_stage(1, None), make_stage(3, None)],
        };
        assert!(selection.validate().is_err());
    }

    #[test]
    fn stage_transition_stops_at_final_stage() {
        let selection = StageSelection {
            stages: vec![make_stage(1, Some(-10.0))],
        };
        assert_eq!(
            selection
                .transition_for_energy(StageIndex(1), -20.0)
                .unwrap(),
            StageTransition::Stop
        );
    }

    #[test]
    fn stage_transition_uses_refine_threshold_as_native_gate() {
        let selection = StageSelection {
            stages: vec![make_stage(1, Some(-5.0)), make_stage(2, None)],
        };
        assert_eq!(
            selection
                .transition_for_energy(StageIndex(1), -6.0)
                .unwrap(),
            StageTransition::Continue
        );
        assert_eq!(
            selection
                .transition_for_energy(StageIndex(1), -4.0)
                .unwrap(),
            StageTransition::Stop
        );
    }

    #[test]
    fn stage_without_threshold_continues_when_next_stage_exists() {
        let selection = StageSelection {
            stages: vec![make_stage(1, None), make_stage(2, None)],
        };
        assert_eq!(
            selection
                .transition_for_energy(StageIndex(1), 100.0)
                .unwrap(),
            StageTransition::Continue
        );
    }

    #[test]
    fn stage_engine_parses_native_scott_codes() {
        assert_eq!(StageEngine::from_native_code('g'), Some(StageEngine::Gulp));
        assert_eq!(StageEngine::from_native_code('C'), Some(StageEngine::Cp2k));
        assert_eq!(
            StageEngine::from_native_code('R'),
            Some(StageEngine::Crystal)
        );
        assert_eq!(StageEngine::from_native_code('A'), Some(StageEngine::Aims));
        assert_eq!(StageEngine::from_native_code('V'), Some(StageEngine::Vasp));
        assert_eq!(
            StageEngine::from_native_code('N'),
            Some(StageEngine::Nwchem)
        );
        assert_eq!(StageEngine::from_native_code('D'), Some(StageEngine::Dmol));
        assert_eq!(
            StageEngine::from_native_code('X'),
            Some(StageEngine::NoEvalExport)
        );
        assert_eq!(StageEngine::from_native_code('?'), None);
    }

    #[test]
    fn final_stage_failure_policy_has_explicit_keep_and_reject_modes() {
        assert_eq!(
            FinalStageFailurePolicy::KeepPreviousAccepted,
            FinalStageFailurePolicy::KeepPreviousAccepted
        );
        assert_eq!(
            FinalStageFailurePolicy::RejectCandidate,
            FinalStageFailurePolicy::RejectCandidate
        );
    }

    #[test]
    fn final_stage_failure_policy_maps_native_only_2nd_energy_flag() {
        assert_eq!(
            FinalStageFailurePolicy::from_only_2nd_energy(false),
            FinalStageFailurePolicy::KeepPreviousAccepted
        );
        assert_eq!(
            FinalStageFailurePolicy::from_only_2nd_energy(true),
            FinalStageFailurePolicy::RejectCandidate
        );
    }

    #[test]
    fn stage_selection_can_be_built_from_native_scott_arrays() {
        let selection = StageSelection::from_native_scott(
            &['G', 'A'],
            &[Some(-10.0), None],
            &[None, Some(-20.0)],
            &[Some(-1.0), None],
            true,
        )
        .unwrap();

        assert_eq!(selection.stages.len(), 2);
        assert_eq!(selection.stages[0].engine, StageEngine::Gulp);
        assert_eq!(selection.stages[1].engine, StageEngine::Aims);
        assert_eq!(selection.stages[0].refine_if_energy_below, Some(-10.0));
        assert_eq!(selection.stages[1].energy_min_threshold, Some(-20.0));
        assert_eq!(selection.stages[0].energy_max_threshold, Some(-1.0));
        assert!(selection.stages[1].keep_only_if_final_stage);
    }

    #[test]
    fn stage_selection_rejects_mismatched_native_input_lengths() {
        let error = StageSelection::from_native_scott(
            &['G', 'A'],
            &[Some(-10.0)],
            &[None, None],
            &[None, None],
            false,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            super::StageSelectionError::MismatchedNativeInputLengths { .. }
        ));
    }
}
