use anyhow::Result;
use patina_driver::application::ports::{
    SurfaceGenerationArtifactSink, SurfaceGenerationPort, SurfaceGenerationRequest,
    SurfacePolarityArtifactSink, SurfacePolarityPort, SurfacePolarityRequest,
};
use patina_driver::application::surface_generation::{
    run_surface_generation_workflow, SurfaceGenerationExecution,
};
use patina_driver::application::surface_polarity::{
    run_surface_polarity_workflow, SurfacePolarityExecution,
};
use patina_surface::{
    MillerIndex, SlabReductionConfig, SurfaceAtom, SurfaceGenerationConfig,
    SurfaceGenerationResult, SurfacePolarityClass, SurfacePolarityReport,
    SurfaceReconstructionMode, SurfaceSlab, SurfaceSupercellConfig, SurfaceTerminationBias,
    SurfaceTopologyDiagnostics,
};
use patina_types::Candidate;
use std::sync::Mutex;

#[derive(Default)]
struct RecordingSink {
    writes: Mutex<Vec<String>>,
}

impl SurfaceGenerationArtifactSink for RecordingSink {
    fn persist_surface_generation_run(&self, execution: &SurfaceGenerationExecution) -> Result<()> {
        self.writes
            .lock()
            .expect("lock")
            .push(execution.result.slab.label.clone());
        Ok(())
    }
}

impl SurfacePolarityArtifactSink for RecordingSink {
    fn persist_surface_polarity_run(&self, execution: &SurfacePolarityExecution) -> Result<()> {
        self.writes
            .lock()
            .expect("lock")
            .push(execution.slab.label.clone());
        Ok(())
    }
}

struct StubSurfacePort;

impl SurfaceGenerationPort for StubSurfacePort {
    fn generate_surface(
        &self,
        request: &SurfaceGenerationRequest,
    ) -> Result<SurfaceGenerationResult> {
        Ok(SurfaceGenerationResult {
            slab: SurfaceSlab {
                label: format!("{}__surface", request.parent.label),
                parent_label: request.parent.label.clone(),
                miller: request.config.miller.clone(),
                lattice: request.parent.lattice,
                periodic_axes: [true, true, false],
                atoms: vec![SurfaceAtom {
                    species: request.parent.atoms[0].species.clone(),
                    fractional: [0.1, 0.2, 0.3],
                    cartesian: [1.0, 2.0, 3.0],
                    source_fractional: Some(request.parent.atoms[0].fractional),
                }],
                thickness_angstrom: request.config.thickness_angstrom,
                vacuum_angstrom: request.config.vacuum_angstrom,
            },
            diagnostics: SurfaceTopologyDiagnostics {
                topology_safe_cut: Some(true),
                broken_bond_estimate: None,
                dedup_report: None,
                chosen_cut_offset_angstrom: Some(0.5),
                interplanar_spacing_angstrom: Some(2.0),
                layer_count: Some(4),
                graph_diagnostics: None,
                surface_bond_summary: None,
                warnings: Vec::new(),
            },
        })
    }
}

struct StubSurfacePolarityPort;

impl SurfacePolarityPort for StubSurfacePolarityPort {
    fn analyze_surface_polarity(
        &self,
        request: &SurfacePolarityRequest,
    ) -> Result<SurfacePolarityReport> {
        Ok(SurfacePolarityReport {
            classification: SurfacePolarityClass::Polar,
            residual_dipole_proxy_z: Some(request.slab.atoms.len() as f64),
            top_species_counts: vec![("O".into(), 1)],
            bottom_species_counts: vec![("Zn".into(), 1)],
            warnings: Vec::new(),
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

fn sample_slab() -> SurfaceSlab {
    SurfaceSlab {
        label: "slab".into(),
        parent_label: "framework".into(),
        miller: MillerIndex { h: 1, k: 0, l: 0 },
        lattice: [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]],
        periodic_axes: [true, true, false],
        atoms: vec![SurfaceAtom {
            species: "Zn".into(),
            fractional: [0.0, 0.0, 0.25],
            cartesian: [0.0, 0.0, 5.0],
            source_fractional: None,
        }],
        thickness_angstrom: 0.0,
        vacuum_angstrom: 0.0,
    }
}

#[test]
fn surface_generation_workflow_runs_through_port_and_sink() {
    let sink = RecordingSink::default();

    let execution = run_surface_generation_workflow(
        sample_candidate(),
        SurfaceGenerationConfig {
            miller: MillerIndex::new(1, 0, 0).expect("miller"),
            thickness_angstrom: 8.0,
            vacuum_angstrom: 10.0,
            supercell: SurfaceSupercellConfig::default(),
            cut_strategy: patina_surface::SurfaceCutStrategy::TopologyAware,
            cut_offset_fraction: None,
            slab_reduction: SlabReductionConfig::default(),
            reconstruction: SurfaceReconstructionMode::None,
            termination_bias: SurfaceTerminationBias::Neutral,
        },
        &StubSurfacePort,
        &sink,
    )
    .expect("workflow");

    assert_eq!(execution.result.slab.periodic_axes, [true, true, false]);
    assert_eq!(
        sink.writes.lock().expect("lock").as_slice(),
        ["framework__surface"]
    );
}

#[test]
fn surface_polarity_workflow_runs_through_port_and_sink() {
    let sink = RecordingSink::default();

    let execution = run_surface_polarity_workflow(sample_slab(), &StubSurfacePolarityPort, &sink)
        .expect("workflow");

    assert_eq!(execution.report.classification, SurfacePolarityClass::Polar);
    assert_eq!(sink.writes.lock().expect("lock").as_slice(), ["slab"]);
}
