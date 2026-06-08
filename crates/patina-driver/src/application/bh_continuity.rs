use anyhow::Result;
use patina_external::GotParser;
use patina_types::{
    BhMoveClassRecord, BhWalkerScientificState, BhWalkerState, EvalResult, EvaluationRecord,
    MoveClassActivationRecord, WalkerRestartArtifactKind, WalkerRestartEquivalenceRecord,
    WalkerRestartState, WalkerStartSource,
};
use std::path::Path;

pub struct BhWalkerStateInput<'a> {
    pub walker_id: &'a str,
    pub step: usize,
    pub restart: WalkerRestartState,
    pub current: &'a EvalResult,
    pub current_output_path: Option<&'a Path>,
    pub best: &'a EvalResult,
    pub best_output_path: Option<&'a Path>,
    pub move_class_activation: Option<MoveClassActivationRecord>,
    pub restart_equivalence: Option<WalkerRestartEquivalenceRecord>,
    pub scientific_state: Option<BhWalkerScientificState>,
}

pub fn build_bh_walker_state(input: BhWalkerStateInput<'_>) -> BhWalkerState {
    BhWalkerState {
        walker_id: input.walker_id.to_string(),
        restart: input.restart,
        current: evaluation_record(input.current, input.current_output_path),
        best: evaluation_record(input.best, input.best_output_path),
        step: input.step,
        move_class_activation: input.move_class_activation,
        restart_equivalence: input.restart_equivalence,
        scientific_state: input.scientific_state,
    }
}

#[cfg(test)]
pub fn load_bh_walker_state_from_gout_paths(
    walker_id: &str,
    origin_label: &str,
    step: usize,
    current_output_path: &Path,
    best_output_path: Option<&Path>,
) -> Result<BhWalkerState> {
    load_bh_walker_state_from_restart_paths(
        walker_id,
        origin_label,
        step,
        current_output_path,
        best_output_path,
        None,
    )
}

pub fn load_bh_walker_state_from_restart_paths(
    walker_id: &str,
    origin_label: &str,
    step: usize,
    current_output_path: &Path,
    best_output_path: Option<&Path>,
    restart_snapshot_path: Option<&Path>,
) -> Result<BhWalkerState> {
    let current = GotParser::parse_file(current_output_path)?;
    let best_path = best_output_path.unwrap_or(current_output_path);
    let best = if best_path == current_output_path {
        current.clone()
    } else {
        GotParser::parse_file(best_path)?
    };
    let restart_equivalence = restart_snapshot_path
        .map(|path| -> Result<WalkerRestartEquivalenceRecord> {
            let restored = super::scott_topology_export::parse_candidate_snapshot(path)?;
            Ok(patina_search::build_walker_restart_equivalence(
                WalkerRestartArtifactKind::WalkerCan,
                Some(path.display().to_string()),
                &restored,
                &current.relaxed_candidate,
                &best.relaxed_candidate,
            ))
        })
        .transpose()?;
    let restart_origin_label = restart_equivalence
        .as_ref()
        .map(|record| record.restored_structure.label.clone())
        .unwrap_or_else(|| origin_label.to_string());
    let restart_source_path = restart_equivalence
        .as_ref()
        .and_then(|record| record.artifact_path.clone())
        .or_else(|| Some(current_output_path.display().to_string()));
    Ok(build_bh_walker_state(BhWalkerStateInput {
        walker_id,
        step,
        restart: WalkerRestartState {
            source: WalkerStartSource::RestartArtifact,
            origin_label: restart_origin_label,
            restored_step: step,
            source_output_path: restart_source_path,
        },
        current: &current,
        current_output_path: Some(current_output_path),
        best: &best,
        best_output_path: Some(best_path),
        move_class_activation: None,
        restart_equivalence,
        scientific_state: None,
    }))
}

pub fn fresh_bh_restart_state(origin_label: impl Into<String>) -> WalkerRestartState {
    WalkerRestartState {
        source: WalkerStartSource::FreshSeed,
        origin_label: origin_label.into(),
        restored_step: 0,
        source_output_path: None,
    }
}

pub fn bh_move_class_record(move_class: patina_search::BhMoveClass) -> BhMoveClassRecord {
    match move_class {
        patina_search::BhMoveClass::MonteCarlo => BhMoveClassRecord::MonteCarlo,
        patina_search::BhMoveClass::SwapCations => BhMoveClassRecord::SwapCations,
        patina_search::BhMoveClass::SwapAtoms => BhMoveClassRecord::SwapAtoms,
        patina_search::BhMoveClass::MutateCluster => BhMoveClassRecord::MutateCluster,
        patina_search::BhMoveClass::TwistCluster => BhMoveClassRecord::TwistCluster,
        patina_search::BhMoveClass::TranslateCluster => BhMoveClassRecord::TranslateCluster,
        patina_search::BhMoveClass::RotateCluster => BhMoveClassRecord::RotateCluster,
    }
}

fn evaluation_record(result: &EvalResult, output_path: Option<&Path>) -> EvaluationRecord {
    let mut record = EvaluationRecord::from(result);
    record.primary_output_path = output_path.map(|path| path.display().to_string());
    record
}

#[cfg(test)]
mod tests {
    use super::{
        build_bh_walker_state, fresh_bh_restart_state, load_bh_walker_state_from_gout_paths,
        load_bh_walker_state_from_restart_paths, BhWalkerStateInput,
    };
    use patina_types::{Candidate, EvalResult};
    use std::fs;
    use std::time::Duration;
    use tempfile::tempdir;

    fn sample_result(label: &str, energy: f64) -> EvalResult {
        EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]; 2],
            relaxed_candidate: Candidate {
                species: vec!["Mg".into(), "O".into()],
                fractional_coords: vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
                lattice: None,
                periodic_axes: [false, false, false],
                label: label.to_string(),
            },
            converged: true,
            wall_time: Duration::from_secs(1),
        }
    }

    #[test]
    fn build_bh_walker_state_preserves_current_and_best() {
        let current = sample_result("current", -1.0);
        let best = sample_result("best", -2.0);
        let state = build_bh_walker_state(BhWalkerStateInput {
            walker_id: "0",
            step: 4,
            restart: fresh_bh_restart_state("seed"),
            current: &current,
            current_output_path: None,
            best: &best,
            best_output_path: None,
            move_class_activation: None,
            restart_equivalence: None,
            scientific_state: None,
        });
        assert_eq!(state.walker_id, "0");
        assert_eq!(state.step, 4);
        assert_eq!(state.restart.origin_label, "seed");
        assert_eq!(state.current.energy, -1.0);
        assert_eq!(state.best.energy, -2.0);
        assert!(state.scientific_state.is_none());
    }

    #[test]
    fn load_bh_walker_state_from_gout_paths_parses_structures() {
        let temp = tempdir().expect("tempdir");
        let current_path = temp.path().join("A2_save.gout");
        let best_path = temp.path().join("A1_save.gout");
        fs::write(
            &current_path,
            "Final energy = -1.25 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Mg 0.0 0.0 0.0\n 2 O 0.5 0.5 0.5\n",
        )
        .expect("write current gout");
        fs::write(
            &best_path,
            "Final energy = -1.50 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Mg 0.1 0.0 0.0\n 2 O 0.5 0.6 0.5\n",
        )
        .expect("write best gout");

        let state = load_bh_walker_state_from_gout_paths(
            "0",
            "restart_seed",
            2,
            &current_path,
            Some(&best_path),
        )
        .expect("load walker state");
        assert_eq!(state.current.energy, -1.25);
        assert_eq!(state.best.energy, -1.50);
        assert_eq!(state.restart.origin_label, "restart_seed");
        assert_eq!(
            state.current.primary_output_path.as_deref(),
            Some(current_path.to_string_lossy().as_ref())
        );
        assert_eq!(
            state.best.primary_output_path.as_deref(),
            Some(best_path.to_string_lossy().as_ref())
        );
        assert!(state.restart_equivalence.is_none());
    }

    #[test]
    fn load_bh_walker_state_from_restart_paths_tracks_walker_can_equivalence() {
        let temp = tempdir().expect("tempdir");
        let restart_dir = temp.path().join("restart");
        fs::create_dir_all(&restart_dir).expect("restart dir");
        let current_path = temp.path().join("A2_save.gout");
        let best_path = temp.path().join("A1_save.gout");
        let walker_can_path = restart_dir.join("walker.can");
        fs::write(
            &current_path,
            "Final energy = -1.25 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Mg 0.0 0.0 0.0\n 2 O 0.5 0.5 0.5\n",
        )
        .expect("write current gout");
        fs::write(
            &best_path,
            "Final energy = -1.50 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Mg 0.1 0.0 0.0\n 2 O 0.5 0.6 0.5\n",
        )
        .expect("write best gout");
        fs::write(
            &walker_can_path,
            "A0001\n1 0.0\n-10.0 0.0 0.0\n0.1 0.2 0.3\n0.0 0.0 0.0\n0.0 0.0 0.0\n2\nMg\nc\n0.0 0.0 0.0\n0.0 0.0 0.0\n1\n0.0\n0.0\nO\nc\n0.5 0.5 0.5\n0.5 0.5 0.5\n2\n0.0\n0.0\n",
        )
        .expect("write walker can");

        let state = load_bh_walker_state_from_restart_paths(
            "0",
            "restart_seed",
            2,
            &current_path,
            Some(&best_path),
            Some(&walker_can_path),
        )
        .expect("load walker state");
        let restart_equivalence = state
            .restart_equivalence
            .as_ref()
            .expect("restart equivalence");
        assert_eq!(
            state.restart.source_output_path.as_deref(),
            Some(walker_can_path.to_string_lossy().as_ref())
        );
        assert_eq!(state.restart.origin_label, "walker");
        assert!(restart_equivalence.matches_current);
        assert!(!restart_equivalence.matches_best);
    }
}
