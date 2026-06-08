use anyhow::Result;
use patina_surface::{SurfacePolarityReport, SurfaceSlab};
use serde::Serialize;

use super::ports::{SurfacePolarityArtifactSink, SurfacePolarityPort, SurfacePolarityRequest};

#[derive(Debug, Clone, Serialize)]
pub struct SurfacePolarityExecution {
    pub slab: SurfaceSlab,
    pub report: SurfacePolarityReport,
}

pub fn run_surface_polarity_workflow(
    slab: SurfaceSlab,
    evaluator: &impl SurfacePolarityPort,
    sink: &impl SurfacePolarityArtifactSink,
) -> Result<SurfacePolarityExecution> {
    let report =
        evaluator.analyze_surface_polarity(&SurfacePolarityRequest { slab: slab.clone() })?;
    let execution = SurfacePolarityExecution { slab, report };
    sink.persist_surface_polarity_run(&execution)?;
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use super::{run_surface_polarity_workflow, SurfacePolarityExecution};
    use crate::application::ports::{
        SurfacePolarityArtifactSink, SurfacePolarityPort, SurfacePolarityRequest,
    };
    use anyhow::Result;
    use patina_surface::{
        MillerIndex, SurfaceAtom, SurfacePolarityClass, SurfacePolarityReport, SurfaceSlab,
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        writes: Mutex<Vec<String>>,
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
    fn workflow_runs_polarity_analysis_and_persists_artifacts() {
        let sink = RecordingSink::default();
        let execution =
            run_surface_polarity_workflow(sample_slab(), &StubSurfacePolarityPort, &sink)
                .expect("workflow");
        assert_eq!(execution.report.classification, SurfacePolarityClass::Polar);
        assert_eq!(sink.writes.lock().expect("lock").as_slice(), ["slab"]);
    }
}
