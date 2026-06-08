use anyhow::Result;
use patina_raspa::{GcmcRequest, GcmcResult, PeriodicFramework};
use patina_types::Candidate;
use serde::Serialize;

use super::ports::{FrameworkGcmcArtifactSink, FrameworkGcmcPort};

#[derive(Debug, Clone, Serialize)]
pub struct FrameworkGcmcExecution {
    pub source_candidate: Candidate,
    pub framework: PeriodicFramework,
    pub request: GcmcRequest,
    pub result: GcmcResult,
}

pub fn run_framework_gcmc_workflow(
    source_candidate: Candidate,
    mut request: GcmcRequest,
    evaluator: &impl FrameworkGcmcPort,
    sink: &impl FrameworkGcmcArtifactSink,
) -> Result<FrameworkGcmcExecution> {
    let framework = PeriodicFramework::try_from_candidate(&source_candidate)?;
    request.framework = framework.clone();
    let result = evaluator.run_framework_gcmc(&request)?;
    let execution = FrameworkGcmcExecution {
        source_candidate,
        framework,
        request,
        result,
    };
    sink.persist_framework_gcmc_run(&execution)?;
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use super::{run_framework_gcmc_workflow, FrameworkGcmcExecution};
    use crate::application::ports::{FrameworkGcmcArtifactSink, FrameworkGcmcPort};
    use anyhow::Result;
    use patina_raspa::{
        DensityGrid, DensityGridBinning, DensityGridNormalization, DensityGridSpec,
        EnergyHistogram, EnergyHistogramSpec, GcmcConditions, GcmcMoveSchedule, GcmcRequest,
        GcmcResult, GcmcSummary, NumberHistogram, NumberHistogramSpec, PeriodicFramework,
        RigidGuestTemplate,
    };
    use patina_types::Candidate;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        writes: Mutex<Vec<String>>,
    }

    impl FrameworkGcmcArtifactSink for RecordingSink {
        fn persist_framework_gcmc_run(&self, execution: &FrameworkGcmcExecution) -> Result<()> {
            self.writes
                .lock()
                .expect("lock")
                .push(execution.framework.label.clone());
            Ok(())
        }
    }

    struct StubPort;

    impl FrameworkGcmcPort for StubPort {
        fn run_framework_gcmc(&self, request: &GcmcRequest) -> Result<GcmcResult> {
            Ok(GcmcResult {
                summary: GcmcSummary {
                    attempted_cycles: request.initialization_cycles + request.production_cycles,
                    accepted_insertions: 1,
                    accepted_deletions: 0,
                    accepted_translations: 1,
                    accepted_rotations: 0,
                    accepted_reinsertions: 0,
                    accepted_widom: 0,
                    rejected_moves: 0,
                    mean_loading: 1.0,
                    mean_energy: -0.5,
                },
                final_candidate: request.framework.to_candidate(),
                density_grid: Some(DensityGrid {
                    dimensions: [4, 4, 4],
                    channels: 1,
                    samples: 1,
                    values: vec![0.0; 64],
                }),
                energy_histogram: Some(EnergyHistogram {
                    total: vec![1.0; 8],
                    vdw: vec![1.0; 8],
                    coulomb: vec![0.0; 8],
                    polarization: vec![0.0; 8],
                }),
                number_histogram: Some(NumberHistogram {
                    per_component: vec![vec![1.0, 0.0, 0.0]],
                }),
                production_trace: Vec::new(),
            })
        }
    }

    fn sample_candidate() -> Candidate {
        Candidate::fully_periodic(
            "framework",
            vec!["Mg".into()],
            vec![[0.0, 0.0, 0.0]],
            [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]],
        )
    }

    #[test]
    fn workflow_runs_and_persists_artifacts() {
        let sink = RecordingSink::default();
        let execution = run_framework_gcmc_workflow(
            sample_candidate(),
            GcmcRequest {
                framework: PeriodicFramework::try_from_candidate(&sample_candidate())
                    .expect("framework"),
                guest_name: "h2".into(),
                guest_template: RigidGuestTemplate::hydrogen(),
                blocks: 1,
                initialization_cycles: 5,
                production_cycles: 10,
                conditions: GcmcConditions {
                    temperature_kelvin: 298.0,
                    chemical_potential: None,
                    fugacity_pascal: None,
                    pressure_pascal: Some(1.0e5),
                },
                move_schedule: GcmcMoveSchedule::default(),
                random_seed: 7,
                max_guest_count: 4,
                minimum_guest_host_distance: 1.0,
                minimum_guest_guest_distance: 1.0,
                density_grid: Some(DensityGridSpec {
                    dimensions: [4, 4, 4],
                    sample_every: 1,
                    write_every: 10,
                    normalization: DensityGridNormalization::Max,
                    binning: DensityGridBinning::Standard,
                    pseudo_atom_channels: vec!["h2".into()],
                }),
                energy_histogram: Some(EnergyHistogramSpec {
                    number_of_bins: 8,
                    range: (-10.0, 10.0),
                    sample_every: 1,
                    write_every: 10,
                }),
                number_histogram: Some(NumberHistogramSpec {
                    lower_limit: 0,
                    upper_limit: 2,
                    sample_every: 1,
                    write_every: 10,
                }),
            },
            &StubPort,
            &sink,
        )
        .expect("workflow");

        assert_eq!(execution.result.summary.accepted_insertions, 1);
        assert_eq!(sink.writes.lock().expect("lock").as_slice(), ["framework"]);
    }
}
