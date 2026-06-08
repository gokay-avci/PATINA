use std::path::{Path, PathBuf};

use crate::domain::bh::WalkerTraceRow;
use crate::domain::ga::{
    ArtifactIndex, ControllerTraceRow, GenerationMetricRow, GenerationStateFile, GenerationStore,
};
use crate::domain::run_manifest::RunManifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowKind {
    Ga,
    Bh,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct RunSnapshot {
    pub run_dir: PathBuf,
    pub loaded_from_run_dir: bool,
    pub manifest: RunManifest,
    pub workflow_kind: WorkflowKind,
    pub generation_metrics: Vec<GenerationMetricRow>,
    pub controller_trace: Vec<ControllerTraceRow>,
    pub walker_trace: Vec<WalkerTraceRow>,
    pub generations: GenerationStore,
    pub artifacts: ArtifactIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunFingerprint {
    pub workflow_kind: WorkflowKind,
    pub latest_generation: Option<usize>,
    pub metric_rows: usize,
    pub controller_rows: usize,
    pub walker_rows: usize,
    pub state_rows: usize,
    pub summary_rows: usize,
    pub raw_files: usize,
    pub output_files: usize,
}

impl RunSnapshot {
    pub fn workspace(root: PathBuf) -> Self {
        Self {
            run_dir: root,
            loaded_from_run_dir: false,
            manifest: RunManifest::default(),
            workflow_kind: WorkflowKind::Unknown,
            generation_metrics: Vec::new(),
            controller_trace: Vec::new(),
            walker_trace: Vec::new(),
            generations: GenerationStore::default(),
            artifacts: ArtifactIndex {
                manifest_path: PathBuf::new(),
                raw_files: Vec::new(),
                output_files: Vec::new(),
            },
        }
    }

    pub fn has_loaded_run(&self) -> bool {
        self.loaded_from_run_dir
    }

    pub fn latest_generation_index(&self) -> Option<usize> {
        self.generation_metrics
            .last()
            .map(|row| row.generation)
            .or_else(|| {
                self.generations
                    .states
                    .keys()
                    .next_back()
                    .copied()
                    .or_else(|| self.generations.summaries.keys().next_back().copied())
            })
    }

    pub fn selected_metric(&self, index: usize) -> Option<&GenerationMetricRow> {
        self.generation_metrics.get(index)
    }

    pub fn generation_state(&self, generation: usize) -> Option<&GenerationStateFile> {
        self.generations.states.get(&generation)
    }

    pub fn generation_summary(
        &self,
        generation: usize,
    ) -> Option<&crate::domain::ga::GenerationSummaryRow> {
        self.generations.summaries.get(&generation)
    }

    pub fn controller_rows_for_generation(
        &self,
        generation: usize,
    ) -> impl Iterator<Item = &ControllerTraceRow> {
        self.controller_trace
            .iter()
            .filter(move |row| row.generation == generation)
    }

    pub fn all_artifact_paths(&self) -> Vec<&Path> {
        if !self.loaded_from_run_dir {
            return Vec::new();
        }
        let mut paths = Vec::with_capacity(
            1 + self.artifacts.raw_files.len() + self.artifacts.output_files.len(),
        );
        paths.push(self.artifacts.manifest_path.as_path());
        for path in &self.artifacts.raw_files {
            paths.push(path.as_path());
        }
        for path in &self.artifacts.output_files {
            paths.push(path.as_path());
        }
        paths
    }

    pub fn fingerprint(&self) -> RunFingerprint {
        RunFingerprint {
            workflow_kind: self.workflow_kind,
            latest_generation: self.latest_generation_index(),
            metric_rows: self.generation_metrics.len(),
            controller_rows: self.controller_trace.len(),
            walker_rows: self.walker_trace.len(),
            state_rows: self.generations.states.len(),
            summary_rows: self.generations.summaries.len(),
            raw_files: self.artifacts.raw_files.len(),
            output_files: self.artifacts.output_files.len(),
        }
    }
}
