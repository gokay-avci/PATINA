use serde::{Deserialize, Serialize};

/// Normalized resource shape for scheduler-side planning.
///
/// This shape is intentionally richer than the current Scott runtime envelope because cluster
/// placement decisions often need more information than the first local implementations do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceShape {
    pub nodes: usize,
    pub cores: usize,
    pub mpi_ranks: usize,
    pub omp_threads_per_rank: usize,
    pub memory_mb: usize,
    pub gpus: usize,
    pub walltime_minutes: usize,
    pub scratch_mb: usize,
}

impl Default for ResourceShape {
    fn default() -> Self {
        Self {
            nodes: 1,
            cores: 1,
            mpi_ranks: 1,
            omp_threads_per_rank: 1,
            memory_mb: 0,
            gpus: 0,
            walltime_minutes: 60,
            scratch_mb: 0,
        }
    }
}

/// Snapshot of what one `ulab` worker or allocation can currently offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub worker_id: String,
    pub site_profile: String,
    pub available_cores: usize,
    pub available_memory_mb: usize,
    pub available_gpus: usize,
    pub max_walltime_minutes: Option<usize>,
    pub available_scratch_mb: Option<usize>,
    pub tags: Vec<String>,
}

impl WorkerCapabilities {
    pub fn matches(&self, requested: &ResourceShape, required_tags: &[String]) -> CapabilityMatch {
        let missing_tags = required_tags
            .iter()
            .filter(|tag| !self.tags.iter().any(|owned| owned == *tag))
            .cloned()
            .collect::<Vec<_>>();

        let cores_fit = requested.cores <= self.available_cores;
        let memory_fit =
            requested.memory_mb == 0 || requested.memory_mb <= self.available_memory_mb;
        let gpus_fit = requested.gpus <= self.available_gpus;
        let walltime_fit = self
            .max_walltime_minutes
            .map(|max| requested.walltime_minutes <= max)
            .unwrap_or(true);
        let scratch_fit = self
            .available_scratch_mb
            .map(|max| requested.scratch_mb <= max)
            .unwrap_or(true);

        CapabilityMatch {
            fits: missing_tags.is_empty()
                && cores_fit
                && memory_fit
                && gpus_fit
                && walltime_fit
                && scratch_fit,
            missing_tags,
            cores_fit,
            memory_fit,
            gpus_fit,
            walltime_fit,
            scratch_fit,
        }
    }
}

/// Diagnostic result of matching a task request to a worker/allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMatch {
    pub fits: bool,
    pub missing_tags: Vec<String>,
    pub cores_fit: bool,
    pub memory_fit: bool,
    pub gpus_fit: bool,
    pub walltime_fit: bool,
    pub scratch_fit: bool,
}

#[cfg(test)]
mod tests {
    use super::{ResourceShape, WorkerCapabilities};

    #[test]
    fn worker_capabilities_accept_matching_request() {
        let caps = WorkerCapabilities {
            worker_id: "worker-a".into(),
            site_profile: "archer2".into(),
            available_cores: 128,
            available_memory_mb: 512_000,
            available_gpus: 4,
            max_walltime_minutes: Some(720),
            available_scratch_mb: Some(2_000_000),
            tags: vec!["cpu".into(), "gpu".into(), "aims".into()],
        };
        let requested = ResourceShape {
            cores: 64,
            memory_mb: 64_000,
            gpus: 2,
            walltime_minutes: 120,
            scratch_mb: 100_000,
            ..ResourceShape::default()
        };

        let matched = caps.matches(&requested, &["cpu".into(), "aims".into()]);
        assert!(matched.fits);
        assert!(matched.missing_tags.is_empty());
    }

    #[test]
    fn worker_capabilities_report_missing_tags_and_resource_pressure() {
        let caps = WorkerCapabilities {
            worker_id: "worker-b".into(),
            site_profile: "young".into(),
            available_cores: 16,
            available_memory_mb: 32_000,
            available_gpus: 0,
            max_walltime_minutes: Some(30),
            available_scratch_mb: Some(500),
            tags: vec!["cpu".into()],
        };
        let requested = ResourceShape {
            cores: 32,
            memory_mb: 64_000,
            gpus: 1,
            walltime_minutes: 90,
            scratch_mb: 1_000,
            ..ResourceShape::default()
        };

        let matched = caps.matches(&requested, &["cpu".into(), "vasp".into()]);
        assert!(!matched.fits);
        assert_eq!(matched.missing_tags, vec!["vasp"]);
        assert!(!matched.cores_fit);
        assert!(!matched.memory_fit);
        assert!(!matched.gpus_fit);
        assert!(!matched.walltime_fit);
        assert!(!matched.scratch_fit);
    }
}
