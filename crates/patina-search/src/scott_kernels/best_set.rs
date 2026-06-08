use patina_types::Candidate;

/// Compact configuration for Scott production best-set and topology-skip semantics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProductionBestSetConfig {
    pub max_best_clusters: usize,
    pub best_energy_cutoff: f64,
    pub best_energy_tolerance: f64,
    pub use_top_analysis: bool,
}

/// Ranked production best-set member retained by the Scott kernel.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductionBestEntry {
    pub candidate_label: String,
    pub final_stage: Option<u8>,
    pub energy: f64,
    pub hashkey: Option<String>,
    pub relaxed_candidate: Candidate,
}

/// Matched best-set member returned by topology or best-set comparisons.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductionBestSetMatch {
    pub rank: usize,
    pub entry: ProductionBestEntry,
}

/// Pure Scott best-set decision returned when considering a candidate for promotion.
#[derive(Debug, Clone, PartialEq)]
pub enum ProductionBestSetDecision {
    Rejected,
    MatchedExisting(Box<ProductionBestSetMatch>),
    Inserted { rank: usize },
}

impl ProductionBestSetDecision {
    pub fn rank(&self) -> Option<usize> {
        match self {
            Self::Rejected => None,
            Self::MatchedExisting(existing) => Some(existing.rank),
            Self::Inserted { rank } => Some(*rank),
        }
    }
}

/// Scott production best-set state and pure topology comparison semantics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProductionBestSet {
    entries: Vec<ProductionBestEntry>,
}

impl ProductionBestSet {
    pub fn contains_hashkey(&self, hashkey: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.hashkey.as_deref() == Some(hashkey))
    }

    pub fn find_hashkey_match(&self, hashkey: &str) -> Option<ProductionBestSetMatch> {
        self.entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.hashkey.as_deref() == Some(hashkey))
            .map(|(index, entry)| ProductionBestSetMatch {
                rank: index + 1,
                entry: entry.clone(),
            })
    }

    pub fn compare_topology(
        &self,
        cfg: ProductionBestSetConfig,
        candidate_hashkey: Option<&str>,
    ) -> Option<ProductionBestSetMatch> {
        if !cfg.use_top_analysis {
            return None;
        }
        candidate_hashkey.and_then(|hashkey| self.find_hashkey_match(hashkey))
    }

    fn find_same_stage_energy_match(
        &self,
        cfg: ProductionBestSetConfig,
        entry: &ProductionBestEntry,
    ) -> Option<ProductionBestSetMatch> {
        self.entries
            .iter()
            .enumerate()
            .find(|(_, existing)| {
                existing.final_stage == entry.final_stage
                    && (existing.energy - entry.energy).abs() < cfg.best_energy_tolerance
            })
            .map(|(index, existing)| ProductionBestSetMatch {
                rank: index + 1,
                entry: existing.clone(),
            })
    }

    pub fn entries(&self) -> &[ProductionBestEntry] {
        &self.entries
    }

    pub fn consider(
        &mut self,
        cfg: ProductionBestSetConfig,
        entry: ProductionBestEntry,
    ) -> ProductionBestSetDecision {
        if cfg.max_best_clusters == 0 || entry.energy >= cfg.best_energy_cutoff {
            return ProductionBestSetDecision::Rejected;
        }
        if cfg.use_top_analysis {
            if let Some(hashkey) = entry.hashkey.as_deref() {
                if let Some(existing) = self.find_hashkey_match(hashkey) {
                    return ProductionBestSetDecision::MatchedExisting(Box::new(existing));
                }
            }
        }
        if let Some(existing) = self.find_same_stage_energy_match(cfg, &entry) {
            return ProductionBestSetDecision::MatchedExisting(Box::new(existing));
        }

        let inserted_label = entry.candidate_label.clone();
        let inserted_energy = entry.energy;
        let inserted_hashkey = entry.hashkey.clone();
        self.entries.push(entry);
        self.entries
            .sort_by(|left, right| left.energy.total_cmp(&right.energy));
        if self.entries.len() > cfg.max_best_clusters {
            self.entries.truncate(cfg.max_best_clusters);
        }

        match self.entries.iter().position(|candidate| {
            candidate.candidate_label == inserted_label
                && candidate.energy == inserted_energy
                && candidate.hashkey == inserted_hashkey
        }) {
            Some(index) => ProductionBestSetDecision::Inserted { rank: index + 1 },
            None => ProductionBestSetDecision::Rejected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProductionBestEntry, ProductionBestSet, ProductionBestSetConfig, ProductionBestSetDecision,
    };
    use patina_types::Candidate;

    fn candidate(label: &str) -> Candidate {
        Candidate::cluster(label, vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    #[test]
    fn topology_comparison_respects_hashkey_and_flag() {
        let mut best_set = ProductionBestSet::default();
        let cfg = ProductionBestSetConfig {
            max_best_clusters: 2,
            best_energy_cutoff: -5.0,
            best_energy_tolerance: 1.0e-4,
            use_top_analysis: true,
        };

        let decision = best_set.consider(
            cfg,
            ProductionBestEntry {
                candidate_label: "a".into(),
                final_stage: Some(1),
                energy: -10.0,
                hashkey: Some("hk-1".into()),
                relaxed_candidate: candidate("a"),
            },
        );
        assert_eq!(decision.rank(), Some(1));

        let matched = best_set
            .compare_topology(cfg, Some("hk-1"))
            .expect("matched topology");
        assert_eq!(matched.rank, 1);
        assert_eq!(matched.entry.candidate_label, "a");
        assert!(best_set.compare_topology(cfg, Some("hk-2")).is_none());
        assert!(best_set
            .compare_topology(
                ProductionBestSetConfig {
                    use_top_analysis: false,
                    ..cfg
                },
                Some("hk-1")
            )
            .is_none());
    }

    #[test]
    fn consider_returns_match_insert_and_reject() {
        let cfg = ProductionBestSetConfig {
            max_best_clusters: 2,
            best_energy_cutoff: -5.0,
            best_energy_tolerance: 1.0e-4,
            use_top_analysis: true,
        };
        let mut best_set = ProductionBestSet::default();

        let inserted = best_set.consider(
            cfg,
            ProductionBestEntry {
                candidate_label: "a".into(),
                final_stage: Some(1),
                energy: -10.0,
                hashkey: Some("hk-1".into()),
                relaxed_candidate: candidate("a"),
            },
        );
        let matched = best_set.consider(
            cfg,
            ProductionBestEntry {
                candidate_label: "b".into(),
                final_stage: Some(1),
                energy: -9.0,
                hashkey: Some("hk-1".into()),
                relaxed_candidate: candidate("b"),
            },
        );
        let rejected = best_set.consider(
            cfg,
            ProductionBestEntry {
                candidate_label: "c".into(),
                final_stage: Some(1),
                energy: -1.0,
                hashkey: Some("hk-3".into()),
                relaxed_candidate: candidate("c"),
            },
        );

        assert_eq!(inserted, ProductionBestSetDecision::Inserted { rank: 1 });
        assert!(matches!(
            matched,
            ProductionBestSetDecision::MatchedExisting(existing)
                if existing.rank == 1 && existing.entry.candidate_label == "a"
        ));
        assert_eq!(rejected, ProductionBestSetDecision::Rejected);
        assert_eq!(best_set.entries().len(), 1);
    }

    #[test]
    fn consider_matches_same_stage_energy_even_without_topology() {
        let cfg = ProductionBestSetConfig {
            max_best_clusters: 3,
            best_energy_cutoff: -5.0,
            best_energy_tolerance: 1.0e-4,
            use_top_analysis: false,
        };
        let mut best_set = ProductionBestSet::default();

        let inserted = best_set.consider(
            cfg,
            ProductionBestEntry {
                candidate_label: "a".into(),
                final_stage: Some(2),
                energy: -10.0,
                hashkey: None,
                relaxed_candidate: candidate("a"),
            },
        );
        let matched = best_set.consider(
            cfg,
            ProductionBestEntry {
                candidate_label: "b".into(),
                final_stage: Some(2),
                energy: -10.0 + 5.0e-5,
                hashkey: None,
                relaxed_candidate: candidate("b"),
            },
        );
        let inserted_other_stage = best_set.consider(
            cfg,
            ProductionBestEntry {
                candidate_label: "c".into(),
                final_stage: Some(3),
                energy: -10.0 + 5.0e-5,
                hashkey: None,
                relaxed_candidate: candidate("c"),
            },
        );

        assert_eq!(inserted, ProductionBestSetDecision::Inserted { rank: 1 });
        assert!(matches!(
            matched,
            ProductionBestSetDecision::MatchedExisting(existing)
                if existing.rank == 1 && existing.entry.candidate_label == "a"
        ));
        assert_eq!(
            inserted_other_stage,
            ProductionBestSetDecision::Inserted { rank: 2 }
        );
        assert_eq!(best_set.entries().len(), 2);
    }
}
