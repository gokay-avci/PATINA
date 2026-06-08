use anyhow::{Context, Result};
use patina_perturber::{
    ClusterPerturbationEngine, ConfiguredDuplicateScreeningEngine,
    DefaultClusterPerturbationEngine, DuplicateScreeningConfig, DuplicateScreeningEngine,
    DuplicateScreeningMode, EnvironmentOverlapFingerprintConfig, OverlapMatrixFingerprintEngine,
    StructureFingerprintEngine,
};
use patina_types::Candidate;
use std::fs;
use std::path::PathBuf;

use super::workflow_provenance::{build_workflow_provenance, WorkflowProvenanceSpec};

#[derive(Debug, Clone)]
pub(crate) struct ClusterPerturbationRunRequest {
    pub candidate_json: PathBuf,
    pub run_dir: PathBuf,
    pub system: Option<String>,
    pub count: usize,
    pub sigma: f64,
    pub max_displacement: Option<f64>,
    pub validate_min_distance: Option<f64>,
    pub duplicate_threshold: f64,
    pub include_p_orbitals: bool,
    pub duplicate_screening_mode: DuplicateScreeningMode,
    pub environment_width_cutoff: f64,
    pub environment_max_atoms_in_sphere: usize,
    pub environment_s_orbital_count: usize,
    pub environment_p_orbital_count: usize,
    pub seed: Option<u64>,
}

struct LocalClusterPerturbationPort {
    engine: DefaultClusterPerturbationEngine,
}

impl super::ports::ClusterPerturbationPort for LocalClusterPerturbationPort {
    fn generate_cluster_perturbations(
        &self,
        request: &super::ports::ClusterPerturbationRequest,
    ) -> Result<patina_perturber::PerturbationBatch> {
        Ok(self
            .engine
            .generate(&request.source, request.config, request.count)?)
    }
}

struct LocalStructureFingerprintPort {
    engine: OverlapMatrixFingerprintEngine,
}

impl super::ports::StructureFingerprintPort for LocalStructureFingerprintPort {
    fn fingerprint_structure(
        &self,
        structure: &patina_perturber::ClusterStructure,
    ) -> Result<patina_perturber::FingerprintVector> {
        Ok(self.engine.fingerprint(structure)?)
    }
}

struct LocalDuplicateScreeningPort {
    engine: ConfiguredDuplicateScreeningEngine,
}

impl super::ports::DuplicateScreeningPort for LocalDuplicateScreeningPort {
    fn fingerprint_distance(
        &self,
        left: &patina_perturber::ClusterStructure,
        right: &patina_perturber::ClusterStructure,
    ) -> Result<patina_perturber::FingerprintDistance> {
        Ok(self.engine.distance(left, right)?)
    }

    fn classify_duplicate(
        &self,
        left: &patina_perturber::ClusterStructure,
        right: &patina_perturber::ClusterStructure,
        threshold: f64,
    ) -> Result<patina_perturber::DuplicateDecision> {
        Ok(self.engine.classify_duplicate(left, right, threshold)?)
    }
}

struct LocalClusterPerturbationArtifactSink {
    run_dir: PathBuf,
    system: String,
    source_path: PathBuf,
    include_p_orbitals: bool,
    duplicate_screening: DuplicateScreeningConfig,
}

impl super::ports::ClusterPerturbationArtifactSink for LocalClusterPerturbationArtifactSink {
    fn persist_cluster_perturbation_run(
        &self,
        execution: &super::cluster_perturbation::ClusterPerturbationExecution,
    ) -> Result<()> {
        let raw_dir = self.run_dir.join("raw");
        let outputs_dir = self.run_dir.join("outputs");
        fs::create_dir_all(&raw_dir)
            .with_context(|| format!("failed to create raw dir `{}`", raw_dir.display()))?;
        fs::create_dir_all(&outputs_dir)
            .with_context(|| format!("failed to create outputs dir `{}`", outputs_dir.display()))?;

        let variant_candidates = execution
            .batch
            .variants
            .iter()
            .map(Candidate::from)
            .collect::<Vec<_>>();
        fs::write(
            self.run_dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "run_name": self.run_dir.file_name().and_then(|name| name.to_str()).unwrap_or("cluster_perturbation"),
                "workflow_id": "structure.perturb-cluster",
                "mode": "cluster_perturbation",
                "system": self.system,
                "engine": "Rust Cluster Perturbation (patina-perturber)",
                "source_path": self.source_path,
                "provenance": serde_json::to_value(build_workflow_provenance(WorkflowProvenanceSpec {
                    workflow_id: "structure.perturb-cluster",
                    workflow_owner: "cluster_perturbation",
                    backend_id: None,
                    backend_mode: None,
                    lane_mode: None,
                    duplicate_policy_mode: None,
                    parallel_contract: None,
                    run_dir: Some(self.run_dir.as_path()),
                    workdir: None,
                    source_run_dir: None,
                    source_path: Some(self.source_path.as_path()),
                    candidate_json: Some(self.source_path.as_path()),
                    template_bundle: None,
                })?)?,
                "count": execution.count,
                "duplicate_threshold": execution.duplicate_threshold,
                "reporting_fingerprint": {
                    "mode": "global_overlap",
                    "include_p_orbitals": self.include_p_orbitals
                },
                "duplicate_screening": self.duplicate_screening,
                "summary": {
                    "source_label": execution.source_cluster.label,
                    "source_fingerprint_dimension": execution.source_fingerprint.values.len(),
                    "variant_count": execution.batch.variants.len(),
                    "duplicate_vs_source_count": execution.variant_analyses.iter().filter(|analysis| analysis.duplicate_vs_source.duplicate).count()
                },
                "artifacts": {
                    "source_candidate": "raw/source_candidate.json",
                    "source_cluster": "raw/source_cluster.json",
                    "source_fingerprint": "outputs/source_fingerprint.json",
                    "perturbation_batch": "outputs/perturbation_batch.json",
                    "variant_analyses": "outputs/variant_analyses.json",
                    "variant_candidates": "outputs/variant_candidates.json"
                },
                "config": execution.config
            }))
            .context("failed to serialize cluster perturbation manifest")?,
        )
        .with_context(|| format!("failed to write manifest in `{}`", self.run_dir.display()))?;
        fs::write(
            raw_dir.join("source_candidate.json"),
            serde_json::to_string_pretty(&execution.source_candidate)
                .context("failed to serialize source candidate")?,
        )?;
        fs::write(
            raw_dir.join("source_cluster.json"),
            serde_json::to_string_pretty(&execution.source_cluster)
                .context("failed to serialize source cluster")?,
        )?;
        fs::write(
            outputs_dir.join("source_fingerprint.json"),
            serde_json::to_string_pretty(&execution.source_fingerprint)
                .context("failed to serialize source fingerprint")?,
        )?;
        fs::write(
            outputs_dir.join("perturbation_batch.json"),
            serde_json::to_string_pretty(&execution.batch)
                .context("failed to serialize perturbation batch")?,
        )?;
        fs::write(
            outputs_dir.join("variant_analyses.json"),
            serde_json::to_string_pretty(&execution.variant_analyses)
                .context("failed to serialize variant analyses")?,
        )?;
        fs::write(
            outputs_dir.join("variant_candidates.json"),
            serde_json::to_string_pretty(&variant_candidates)
                .context("failed to serialize variant candidates")?,
        )?;
        Ok(())
    }
}

pub(crate) fn run_cluster_perturbation(
    request: ClusterPerturbationRunRequest,
) -> Result<super::cluster_perturbation::ClusterPerturbationExecution> {
    fs::create_dir_all(&request.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", request.run_dir.display()))?;
    let source_path = crate::absolutize_path(&request.candidate_json)?;
    let candidate = crate::read_candidate_json(&source_path)?;
    let system = request.system.unwrap_or_else(|| candidate.label.clone());
    let duplicate_screening = DuplicateScreeningConfig {
        mode: request.duplicate_screening_mode,
        include_p_orbitals: request.include_p_orbitals,
        environment: EnvironmentOverlapFingerprintConfig {
            width_cutoff: request.environment_width_cutoff,
            max_atoms_in_sphere: request.environment_max_atoms_in_sphere,
            s_orbital_count: request.environment_s_orbital_count,
            p_orbital_count: request.environment_p_orbital_count,
        },
    };
    super::cluster_perturbation::run_cluster_perturbation_workflow(
        super::cluster_perturbation::ClusterPerturbationWorkflowRequest {
            source_candidate: candidate,
            config: patina_perturber::PerturbationConfig {
                sigma: request.sigma,
                max_displacement: request.max_displacement,
                validate_min_distance: request.validate_min_distance,
                seed: request.seed,
                ..patina_perturber::PerturbationConfig::default()
            },
            count: request.count,
            duplicate_threshold: request.duplicate_threshold,
        },
        &LocalClusterPerturbationPort {
            engine: DefaultClusterPerturbationEngine,
        },
        &LocalStructureFingerprintPort {
            engine: OverlapMatrixFingerprintEngine {
                include_p_orbitals: request.include_p_orbitals,
            },
        },
        &LocalDuplicateScreeningPort {
            engine: ConfiguredDuplicateScreeningEngine {
                config: duplicate_screening.clone(),
            },
        },
        &LocalClusterPerturbationArtifactSink {
            run_dir: request.run_dir,
            system,
            source_path,
            include_p_orbitals: request.include_p_orbitals,
            duplicate_screening,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{run_cluster_perturbation, ClusterPerturbationRunRequest};
    use patina_perturber::DuplicateScreeningMode;
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "patina_driver_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_candidate_json(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("cluster.json");
        fs::write(
            &path,
            serde_json::to_string_pretty(&patina_types::Candidate::cluster(
                "cluster",
                vec!["Mg".to_string(), "O".to_string()],
                vec![[0.0, 0.0, 0.0], [1.5, 0.0, 0.0]],
            ))
            .expect("candidate json"),
        )
        .expect("write candidate");
        path
    }

    #[test]
    fn cluster_runtime_manifest_records_environment_assignment_duplicate_mode() {
        let dir = temp_dir("env_assignment_runtime");
        let candidate_json = write_candidate_json(&dir);
        let run_dir = dir.join("run");

        let execution = run_cluster_perturbation(ClusterPerturbationRunRequest {
            candidate_json,
            run_dir: run_dir.clone(),
            system: Some("cluster-system".into()),
            count: 1,
            sigma: 0.1,
            max_displacement: Some(0.2),
            validate_min_distance: Some(0.5),
            duplicate_threshold: 1.0e-6,
            include_p_orbitals: false,
            duplicate_screening_mode: DuplicateScreeningMode::EnvironmentAssignment,
            environment_width_cutoff: 1.0,
            environment_max_atoms_in_sphere: 8,
            environment_s_orbital_count: 1,
            environment_p_orbital_count: 0,
            seed: Some(7),
        })
        .expect("runtime execution");

        assert_eq!(execution.variant_analyses.len(), 1);
        let manifest = fs::read_to_string(run_dir.join("manifest.json")).expect("manifest");
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest).expect("manifest json");
        assert_eq!(
            manifest_json["duplicate_screening"]["mode"],
            serde_json::Value::String("environment_assignment".into())
        );
        assert_eq!(
            manifest_json["reporting_fingerprint"]["mode"],
            serde_json::Value::String("global_overlap".into())
        );

        fs::remove_dir_all(dir).ok();
    }
}
