use patina_types::{
    Candidate, StructureRecord, WalkerRestartArtifactKind, WalkerRestartEquivalenceRecord,
};

/// Builds a typed BH restart-equivalence record from a recovered restart artifact and live state.
pub fn build_walker_restart_equivalence(
    artifact_kind: WalkerRestartArtifactKind,
    artifact_path: Option<String>,
    restored: &Candidate,
    current: &Candidate,
    best: &Candidate,
) -> WalkerRestartEquivalenceRecord {
    WalkerRestartEquivalenceRecord {
        artifact_kind,
        artifact_path,
        restored_structure: StructureRecord::from(restored),
        matches_current: candidates_match_for_restart_equivalence(restored, current),
        matches_best: candidates_match_for_restart_equivalence(restored, best),
    }
}

fn candidates_match_for_restart_equivalence(left: &Candidate, right: &Candidate) -> bool {
    left.species == right.species
        && left.fractional_coords == right.fractional_coords
        && left.lattice == right.lattice
        && left.periodic_axes == right.periodic_axes
}

#[cfg(test)]
mod tests {
    use super::build_walker_restart_equivalence;
    use patina_types::{Candidate, WalkerRestartArtifactKind};

    fn candidate(label: &str, x: f64) -> Candidate {
        Candidate::cluster(label, vec!["Mg".into()], vec![[x, 0.0, 0.0]])
    }

    #[test]
    fn restart_equivalence_tracks_current_and_best_matches() {
        let restored = candidate("walker", 0.0);
        let current = candidate("current", 0.0);
        let best = candidate("best", 1.0);

        let record = build_walker_restart_equivalence(
            WalkerRestartArtifactKind::WalkerCan,
            Some("restart/walker.can".into()),
            &restored,
            &current,
            &best,
        );

        assert_eq!(record.artifact_kind, WalkerRestartArtifactKind::WalkerCan);
        assert_eq!(record.artifact_path.as_deref(), Some("restart/walker.can"));
        assert_eq!(record.restored_structure.label, "walker");
        assert!(record.matches_current);
        assert!(!record.matches_best);
    }

    #[test]
    fn restart_equivalence_ignores_label_differences() {
        let restored = candidate("walker", 0.0);
        let current = candidate("A2_save", 0.0);

        let record = build_walker_restart_equivalence(
            WalkerRestartArtifactKind::WalkerCan,
            None,
            &restored,
            &current,
            &current,
        );

        assert!(record.matches_current);
        assert!(record.matches_best);
        assert_eq!(record.restored_structure.label, "walker");
    }
}
