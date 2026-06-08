use anyhow::Result;

pub trait SurfaceReconstructionArtifactSink {
    fn persist_surface_reconstruction_run(
        &self,
        execution: &crate::application::surface_reconstruction::SurfaceReconstructionExecution,
    ) -> Result<()>;
}
