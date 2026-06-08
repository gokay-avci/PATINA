use anyhow::Result;
use patina_driver::application::framework_gcmc::{
    run_framework_gcmc_workflow, FrameworkGcmcExecution,
};
use patina_driver::application::framework_normalization::{
    run_framework_normalization_workflow, FrameworkNormalizationExecution,
};
use patina_driver::application::framework_symmetry::{
    run_framework_symmetry_workflow, FrameworkSymmetryExecution,
};
use patina_driver::application::ports::{
    FrameworkGcmcArtifactSink, FrameworkGcmcPort, FrameworkNormalizationArtifactSink,
    FrameworkNormalizationPort, FrameworkNormalizationRequest, FrameworkNormalizationTarget,
    FrameworkSymmetryArtifactSink, FrameworkSymmetryPort, FrameworkSymmetryRequest,
};
use patina_raspa::{
    DensityGrid, DensityGridBinning, DensityGridNormalization, DensityGridSpec, EnergyHistogram,
    EnergyHistogramSpec, GcmcConditions, GcmcMoveSchedule, GcmcRequest, GcmcResult, GcmcSummary,
    NumberHistogram, NumberHistogramSpec, PeriodicFramework, RigidGuestTemplate, SymmetryAnalysis,
    SymmetryTolerance,
};
use patina_types::Candidate;
use std::sync::Mutex;

#[derive(Default)]
struct RecordingSink {
    writes: Mutex<Vec<String>>,
}

impl FrameworkSymmetryArtifactSink for RecordingSink {
    fn persist_framework_symmetry_run(&self, execution: &FrameworkSymmetryExecution) -> Result<()> {
        self.writes
            .lock()
            .expect("lock")
            .push(execution.source_candidate.label.clone());
        Ok(())
    }
}

impl FrameworkNormalizationArtifactSink for RecordingSink {
    fn persist_framework_normalization_run(
        &self,
        execution: &FrameworkNormalizationExecution,
    ) -> Result<()> {
        self.writes
            .lock()
            .expect("lock")
            .push(execution.normalized_framework.label.clone());
        Ok(())
    }
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

struct StubSymmetryAnalyzer;

impl FrameworkSymmetryPort for StubSymmetryAnalyzer {
    fn analyze_framework_symmetry(
        &self,
        request: &FrameworkSymmetryRequest,
    ) -> Result<SymmetryAnalysis> {
        Ok(SymmetryAnalysis {
            hall_number: Some(1),
            international_number: Some(1),
            hm_symbol: Some("P1".into()),
            primitive_lattice: Some(request.framework.lattice),
            reduced_lattice: Some(request.framework.lattice),
            symmetrized_atoms: request.framework.atoms.clone(),
            operations: Vec::new(),
            orbits: vec![0; request.framework.atoms.len()],
            wyckoff_letters: vec!["a".into(); request.framework.atoms.len()],
            standardized_framework: Some(request.framework.clone()),
            primitive_standardized_framework: Some(request.framework.clone()),
        })
    }
}

struct StubNormalizer;

impl FrameworkNormalizationPort for StubNormalizer {
    fn normalize_framework(
        &self,
        request: &FrameworkNormalizationRequest,
    ) -> Result<PeriodicFramework> {
        let mut framework = request.framework.clone();
        framework.label = match request.target {
            FrameworkNormalizationTarget::Standardized => "standardized".into(),
            FrameworkNormalizationTarget::PrimitiveStandardized => "primitive_standardized".into(),
        };
        Ok(framework)
    }
}

struct StubGcmcPort;

impl FrameworkGcmcPort for StubGcmcPort {
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

fn sample_gcmc_request() -> GcmcRequest {
    GcmcRequest {
        framework: PeriodicFramework::try_from_candidate(&sample_candidate()).expect("framework"),
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
    }
}

#[test]
fn framework_symmetry_workflow_runs_through_port_and_sink() {
    let sink = RecordingSink::default();

    let execution = run_framework_symmetry_workflow(
        sample_candidate(),
        SymmetryTolerance::default(),
        &StubSymmetryAnalyzer,
        &sink,
    )
    .expect("workflow");

    assert_eq!(execution.analysis.international_number, Some(1));
    assert_eq!(sink.writes.lock().expect("lock").as_slice(), ["framework"]);
}

#[test]
fn framework_normalization_workflow_persists_standardized_result() {
    let sink = RecordingSink::default();

    let execution = run_framework_normalization_workflow(
        sample_candidate(),
        SymmetryTolerance::default(),
        FrameworkNormalizationTarget::Standardized,
        &StubNormalizer,
        &sink,
    )
    .expect("workflow");

    assert_eq!(execution.normalized_framework.label, "standardized");
    assert_eq!(
        sink.writes.lock().expect("lock").as_slice(),
        ["standardized"]
    );
}

#[test]
fn framework_gcmc_workflow_rebinds_framework_and_persists_result() {
    let sink = RecordingSink::default();

    let execution = run_framework_gcmc_workflow(
        sample_candidate(),
        sample_gcmc_request(),
        &StubGcmcPort,
        &sink,
    )
    .expect("workflow");

    assert_eq!(execution.result.summary.accepted_insertions, 1);
    assert_eq!(execution.request.framework.label, "framework");
    assert_eq!(sink.writes.lock().expect("lock").as_slice(), ["framework"]);
}
