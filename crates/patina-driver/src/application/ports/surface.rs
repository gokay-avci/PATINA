use anyhow::Result;

use patina_surface::{
    SurfaceGenerationConfig, SurfaceGenerationResult, SurfaceParentStructure,
    SurfacePolarityReport, SurfaceSlab,
};

#[derive(Debug, Clone)]
pub struct SurfaceGenerationRequest {
    pub parent: SurfaceParentStructure,
    pub config: SurfaceGenerationConfig,
}

pub trait SurfaceGenerationPort {
    fn generate_surface(
        &self,
        request: &SurfaceGenerationRequest,
    ) -> Result<SurfaceGenerationResult>;
}

pub trait SurfaceGenerationArtifactSink {
    fn persist_surface_generation_run(
        &self,
        execution: &crate::application::surface_generation::SurfaceGenerationExecution,
    ) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct SurfacePolarityRequest {
    pub slab: SurfaceSlab,
}

pub trait SurfacePolarityPort {
    fn analyze_surface_polarity(
        &self,
        request: &SurfacePolarityRequest,
    ) -> Result<SurfacePolarityReport>;
}

pub trait SurfacePolarityArtifactSink {
    fn persist_surface_polarity_run(
        &self,
        execution: &crate::application::surface_polarity::SurfacePolarityExecution,
    ) -> Result<()>;
}
