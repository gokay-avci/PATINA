use std::fs;
use std::io;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

use crate::{ExternalBatchReceipt, OrchestratedJobState};

/// Durable JSON-backed projection store for receipts and orchestration state.
///
/// This store exists for recovery, replay, and postmortem inspection. It must not become the
/// hot-path transport between active workers and coordinators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableProjectionStore {
    root: Utf8PathBuf,
}

impl DurableProjectionStore {
    pub fn new(root: impl Into<Utf8PathBuf>) -> Result<Self, ProjectionStoreError> {
        let store = Self { root: root.into() };
        store.ensure_layout()?;
        Ok(store)
    }

    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    pub fn persist_receipt(
        &self,
        receipt: &ExternalBatchReceipt,
    ) -> Result<(), ProjectionStoreError> {
        self.write_json(&self.receipt_path(&receipt.receipt_id), receipt)
    }

    pub fn load_receipt(
        &self,
        receipt_id: &str,
    ) -> Result<Option<ExternalBatchReceipt>, ProjectionStoreError> {
        self.read_optional_json(&self.receipt_path(receipt_id))
    }

    /// Lists receipts for one Scott runtime job.
    ///
    /// This is a durable-path recovery helper, not a hot-path query interface.
    pub fn list_receipts_for_job(
        &self,
        job_id: &str,
    ) -> Result<Vec<ExternalBatchReceipt>, ProjectionStoreError> {
        let mut receipts = Vec::new();
        let dir = self.receipts_dir();
        let entries =
            fs::read_dir(dir.as_std_path()).map_err(|source| ProjectionStoreError::Read {
                path: dir.clone(),
                source,
            })?;

        for entry in entries {
            let entry = entry.map_err(|source| ProjectionStoreError::Read {
                path: dir.clone(),
                source,
            })?;
            let path = utf8_entry_path(&dir, entry.path())?;
            if path.extension() != Some("json") {
                continue;
            }

            let receipt: ExternalBatchReceipt = self
                .read_optional_json(&path)?
                .ok_or_else(|| ProjectionStoreError::MissingProjection { path: path.clone() })?;
            if receipt.job_id == job_id {
                receipts.push(receipt);
            }
        }

        receipts.sort_by(|left, right| {
            left.submitted_at_ms
                .cmp(&right.submitted_at_ms)
                .then_with(|| left.receipt_id.cmp(&right.receipt_id))
        });
        Ok(receipts)
    }

    pub fn persist_state(&self, state: &OrchestratedJobState) -> Result<(), ProjectionStoreError> {
        self.write_json(&self.state_path(&state.job_id), state)
    }

    pub fn load_state(
        &self,
        job_id: &str,
    ) -> Result<Option<OrchestratedJobState>, ProjectionStoreError> {
        self.read_optional_json(&self.state_path(job_id))
    }

    fn ensure_layout(&self) -> Result<(), ProjectionStoreError> {
        for dir in [self.root.clone(), self.receipts_dir(), self.states_dir()] {
            fs::create_dir_all(dir.as_std_path())
                .map_err(|source| ProjectionStoreError::CreateDir { path: dir, source })?;
        }
        Ok(())
    }

    fn receipts_dir(&self) -> Utf8PathBuf {
        self.root.join("receipts")
    }

    fn states_dir(&self) -> Utf8PathBuf {
        self.root.join("states")
    }

    fn receipt_path(&self, receipt_id: &str) -> Utf8PathBuf {
        self.receipts_dir()
            .join(format!("{}.json", storage_key(receipt_id)))
    }

    fn state_path(&self, job_id: &str) -> Utf8PathBuf {
        self.states_dir()
            .join(format!("{}.json", storage_key(job_id)))
    }

    fn read_optional_json<T>(&self, path: &Utf8Path) -> Result<Option<T>, ProjectionStoreError>
    where
        T: DeserializeOwned,
    {
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(path.as_std_path()).map_err(|source| ProjectionStoreError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|source| ProjectionStoreError::Decode {
                path: path.to_path_buf(),
                source,
            })
    }

    fn write_json<T>(&self, path: &Utf8Path, value: &T) -> Result<(), ProjectionStoreError>
    where
        T: Serialize,
    {
        let parent = path.parent().unwrap_or(self.root());
        fs::create_dir_all(parent.as_std_path()).map_err(|source| {
            ProjectionStoreError::CreateDir {
                path: parent.to_path_buf(),
                source,
            }
        })?;

        let bytes =
            serde_json::to_vec_pretty(value).map_err(|source| ProjectionStoreError::Encode {
                path: path.to_path_buf(),
                source,
            })?;
        let temp_path = parent.join(format!(
            ".{}.tmp",
            path.file_name().unwrap_or("projection.json")
        ));

        fs::write(temp_path.as_std_path(), bytes).map_err(|source| {
            ProjectionStoreError::Write {
                path: temp_path.clone(),
                source,
            }
        })?;
        fs::rename(temp_path.as_std_path(), path.as_std_path()).map_err(|source| {
            ProjectionStoreError::Write {
                path: path.to_path_buf(),
                source,
            }
        })?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ProjectionStoreError {
    #[error("failed to create durable projection directory `{path}`: {source}")]
    CreateDir {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read durable projection file `{path}`: {source}")]
    Read {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write durable projection file `{path}`: {source}")]
    Write {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to encode durable projection `{path}`: {source}")]
    Encode {
        path: Utf8PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to decode durable projection `{path}`: {source}")]
    Decode {
        path: Utf8PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("projection entry under `{root}` was not valid UTF-8")]
    NonUtf8Entry { root: Utf8PathBuf },
    #[error("projection file `{path}` disappeared while being listed")]
    MissingProjection { path: Utf8PathBuf },
}

fn utf8_entry_path(
    root: &Utf8Path,
    path: std::path::PathBuf,
) -> Result<Utf8PathBuf, ProjectionStoreError> {
    Utf8PathBuf::from_path_buf(path).map_err(|_| ProjectionStoreError::NonUtf8Entry {
        root: root.to_path_buf(),
    })
}

fn storage_key(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::DurableProjectionStore;
    use crate::{
        ExternalBatchReceipt, OrchestratedJobState, OrchestratedJobStatus, SchedulerFamily,
        SchedulerJobIdentifier, SchedulerReceiptState,
    };

    static TEMP_STORE_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_store() -> (DurableProjectionStore, camino::Utf8PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "patina-ulab-projection-store-{}-{}-{}",
            std::process::id(),
            TEMP_STORE_COUNTER.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be valid")
                .as_nanos()
        ));
        let root = camino::Utf8PathBuf::from_path_buf(root).expect("temp path should be utf-8");
        let store = DurableProjectionStore::new(root.clone()).expect("store should initialize");
        (store, root)
    }

    fn sample_receipt(
        receipt_id: &str,
        job_id: &str,
        submitted_at_ms: u64,
    ) -> ExternalBatchReceipt {
        ExternalBatchReceipt {
            receipt_id: receipt_id.into(),
            job_id: job_id.into(),
            scheduler_job: SchedulerJobIdentifier {
                scheduler_family: SchedulerFamily::Slurm,
                allocation_id: format!("alloc-{receipt_id}"),
                step_id: Some("0".into()),
                array_job_id: None,
                array_index: None,
            },
            state: SchedulerReceiptState::Queued,
            submitted_at_ms,
            last_observed_at_ms: submitted_at_ms,
            launch_host: Some("login-a".into()),
            workdir: Some(format!("/scratch/{job_id}")),
            launcher_provenance: Some(crate::SiteProfile::archer2().launcher_provenance()),
            last_telemetry: None,
        }
    }

    #[test]
    fn projection_store_roundtrips_receipts_and_state() {
        let (store, root) = temp_store();
        let receipt = sample_receipt("receipt-1", "job-1", 100);
        let mut state = OrchestratedJobState::new("job-1", 100);
        state
            .transition(OrchestratedJobStatus::Admitted, 110)
            .unwrap();
        state.batch_receipt_id = Some(receipt.receipt_id.clone());

        store.persist_receipt(&receipt).unwrap();
        store.persist_state(&state).unwrap();

        assert_eq!(store.load_receipt("receipt-1").unwrap(), Some(receipt));
        assert_eq!(store.load_state("job-1").unwrap(), Some(state));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn projection_store_filters_receipts_by_job() {
        let (store, root) = temp_store();
        store
            .persist_receipt(&sample_receipt("receipt-2", "job-a", 200))
            .unwrap();
        store
            .persist_receipt(&sample_receipt("receipt-3", "job-a", 150))
            .unwrap();
        store
            .persist_receipt(&sample_receipt("receipt-4", "job-b", 175))
            .unwrap();

        let receipts = store.list_receipts_for_job("job-a").unwrap();
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].receipt_id, "receipt-3");
        assert_eq!(receipts[1].receipt_id, "receipt-2");

        std::fs::remove_dir_all(root).unwrap();
    }
}
