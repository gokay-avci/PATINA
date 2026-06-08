use anyhow::Result;
use patina_surface::{SurfaceGenerationConfig, SurfaceGenerationResult, SurfaceParentStructure};
use patina_types::Candidate;
use serde::Serialize;

use super::ports::{
    SurfaceGenerationArtifactSink, SurfaceGenerationPort, SurfaceGenerationRequest,
};

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceGenerationExecution {
    pub source_candidate: Candidate,
    pub parent: SurfaceParentStructure,
    pub config: SurfaceGenerationConfig,
    pub result: SurfaceGenerationResult,
}

pub fn run_surface_generation_workflow(
    source_candidate: Candidate,
    config: SurfaceGenerationConfig,
    evaluator: &impl SurfaceGenerationPort,
    sink: &impl SurfaceGenerationArtifactSink,
) -> Result<SurfaceGenerationExecution> {
    let parent = SurfaceParentStructure::try_from_candidate(&source_candidate)?;
    let result = evaluator.generate_surface(&SurfaceGenerationRequest {
        parent: parent.clone(),
        config: config.clone(),
    })?;
    let execution = SurfaceGenerationExecution {
        source_candidate,
        parent,
        config,
        result,
    };
    sink.persist_surface_generation_run(&execution)?;
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use super::{run_surface_generation_workflow, SurfaceGenerationExecution};
    use crate::application::ports::{
        SurfaceGenerationArtifactSink, SurfaceGenerationPort, SurfaceGenerationRequest,
    };
    use anyhow::Result;
    use patina_surface::{
        MillerIndex, SlabReductionConfig, SurfaceAtom, SurfaceGenerationConfig,
        SurfaceGenerationResult, SurfaceReconstructionMode, SurfaceSlab, SurfaceSupercellConfig,
        SurfaceTerminationBias, SurfaceTopologyDiagnostics,
    };
    use patina_types::Candidate;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        writes: Mutex<Vec<String>>,
    }

    impl SurfaceGenerationArtifactSink for RecordingSink {
        fn persist_surface_generation_run(
            &self,
            execution: &SurfaceGenerationExecution,
        ) -> Result<()> {
            self.writes
                .lock()
                .expect("lock")
                .push(execution.result.slab.label.clone());
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

    fn sample_candidate() -> Candidate {
        Candidate::fully_periodic(
            "framework",
            vec!["Mg".into()],
            vec![[0.0, 0.0, 0.0]],
            [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]],
        )
    }

    #[test]
    fn workflow_generates_surface_and_persists_artifacts() {
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
}
