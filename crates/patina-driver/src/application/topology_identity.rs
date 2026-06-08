use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyArchiveRecord {
    pub rank: usize,
    pub candidate_label: String,
    pub hashkey: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopologyIdentityMatch {
    BestArchive(TopologyArchiveRecord),
    Blacklist { hashkey: String },
    ImportedLibrary { hashkey: String, occurrences: usize },
    CurrentRunHistory { hashkey: String, occurrences: usize },
}

#[derive(Debug, Clone, Default)]
pub struct TopologyIdentityStore {
    best_archive_by_hashkey: HashMap<String, TopologyArchiveRecord>,
    blacklist: HashSet<String>,
    imported_library: HashMap<String, usize>,
    current_run_history: HashMap<String, usize>,
}

impl TopologyIdentityStore {
    pub fn from_imported_hashkeys(hashkeys: impl IntoIterator<Item = String>) -> Self {
        let mut store = Self::default();
        for hashkey in hashkeys {
            let trimmed = hashkey.trim();
            if trimmed.is_empty() {
                continue;
            }
            *store
                .imported_library
                .entry(trimmed.to_string())
                .or_insert(0) += 1;
        }
        store
    }

    pub fn probe(&self, hashkey: &str) -> Option<TopologyIdentityMatch> {
        let hashkey = hashkey.trim();
        if hashkey.is_empty() {
            return None;
        }
        if let Some(record) = self.best_archive_by_hashkey.get(hashkey) {
            return Some(TopologyIdentityMatch::BestArchive(record.clone()));
        }
        if self.blacklist.contains(hashkey) {
            return Some(TopologyIdentityMatch::Blacklist {
                hashkey: hashkey.to_string(),
            });
        }
        if let Some(&occurrences) = self.imported_library.get(hashkey) {
            return Some(TopologyIdentityMatch::ImportedLibrary {
                hashkey: hashkey.to_string(),
                occurrences,
            });
        }
        self.current_run_history
            .get(hashkey)
            .copied()
            .map(|occurrences| TopologyIdentityMatch::CurrentRunHistory {
                hashkey: hashkey.to_string(),
                occurrences,
            })
    }

    pub fn replace_best_archive(
        &mut self,
        records: impl IntoIterator<Item = TopologyArchiveRecord>,
    ) {
        let mut archive = HashMap::new();
        for record in records {
            let hashkey = record.hashkey.trim();
            if hashkey.is_empty() {
                continue;
            }
            archive.insert(
                hashkey.to_string(),
                TopologyArchiveRecord {
                    hashkey: hashkey.to_string(),
                    ..record
                },
            );
        }
        self.best_archive_by_hashkey = archive;
    }

    #[cfg(test)]
    pub fn register_blacklist_hashkey(&mut self, hashkey: &str) -> Result<()> {
        let hashkey = hashkey.trim();
        if hashkey.is_empty() {
            return Err(anyhow!("topology blacklist hashkey must not be empty"));
        }
        self.blacklist.insert(hashkey.to_string());
        Ok(())
    }

    pub fn register_current_run_hashkey(&mut self, hashkey: &str) -> Result<()> {
        let hashkey = hashkey.trim();
        if hashkey.is_empty() {
            return Err(anyhow!("topology current-run hashkey must not be empty"));
        }
        *self
            .current_run_history
            .entry(hashkey.to_string())
            .or_insert(0) += 1;
        Ok(())
    }

    pub fn imported_library_records(&self) -> Vec<(String, usize)> {
        let mut records = self
            .imported_library
            .iter()
            .map(|(hashkey, &occurrences)| (hashkey.clone(), occurrences))
            .collect::<Vec<_>>();
        records.sort_by(|left, right| left.0.cmp(&right.0));
        records
    }

    pub fn current_run_history_records(&self) -> Vec<(String, usize)> {
        let mut records = self
            .current_run_history
            .iter()
            .map(|(hashkey, &occurrences)| (hashkey.clone(), occurrences))
            .collect::<Vec<_>>();
        records.sort_by(|left, right| left.0.cmp(&right.0));
        records
    }
}

#[cfg(test)]
mod tests {
    use super::{TopologyArchiveRecord, TopologyIdentityMatch, TopologyIdentityStore};

    #[test]
    fn probe_distinguishes_archive_blacklist_imported_and_current_run() {
        let mut store = TopologyIdentityStore::from_imported_hashkeys([
            "hk-imported".to_string(),
            "hk-imported".to_string(),
        ]);
        store.replace_best_archive([TopologyArchiveRecord {
            rank: 1,
            candidate_label: "accepted".into(),
            hashkey: "hk-archive".into(),
        }]);
        store
            .register_blacklist_hashkey("hk-black")
            .expect("blacklist");
        store
            .register_current_run_hashkey("hk-run")
            .expect("current run");

        assert!(matches!(
            store.probe("hk-archive"),
            Some(TopologyIdentityMatch::BestArchive(record))
                if record.rank == 1 && record.candidate_label == "accepted"
        ));
        assert!(matches!(
            store.probe("hk-black"),
            Some(TopologyIdentityMatch::Blacklist { hashkey }) if hashkey == "hk-black"
        ));
        assert!(matches!(
            store.probe("hk-imported"),
            Some(TopologyIdentityMatch::ImportedLibrary { occurrences, .. }) if occurrences == 2
        ));
        assert!(matches!(
            store.probe("hk-run"),
            Some(TopologyIdentityMatch::CurrentRunHistory { occurrences, .. }) if occurrences == 1
        ));
        assert!(store.probe("missing").is_none());
    }
}
