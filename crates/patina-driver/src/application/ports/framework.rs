use anyhow::Result;
use serde::{Deserialize, Serialize};

use patina_raspa::{
    GcmcRequest, GcmcResult, PeriodicFramework, SymmetryAnalysis, SymmetryTolerance,
};

#[derive(Debug, Clone)]
pub struct FrameworkSymmetryRequest {
    pub framework: PeriodicFramework,
    pub tolerance: SymmetryTolerance,
}

pub trait FrameworkSymmetryPort {
    fn analyze_framework_symmetry(
        &self,
        request: &FrameworkSymmetryRequest,
    ) -> Result<SymmetryAnalysis>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameworkNormalizationTarget {
    Standardized,
    PrimitiveStandardized,
}

#[derive(Debug, Clone)]
pub struct FrameworkNormalizationRequest {
    pub framework: PeriodicFramework,
    pub tolerance: SymmetryTolerance,
    pub target: FrameworkNormalizationTarget,
}

pub trait FrameworkNormalizationPort {
    fn normalize_framework(
        &self,
        request: &FrameworkNormalizationRequest,
    ) -> Result<PeriodicFramework>;
}

pub trait FrameworkSymmetryArtifactSink {
    fn persist_framework_symmetry_run(
        &self,
        execution: &crate::application::framework_symmetry::FrameworkSymmetryExecution,
    ) -> Result<()>;
}

pub trait FrameworkNormalizationArtifactSink {
    fn persist_framework_normalization_run(
        &self,
        execution: &crate::application::framework_normalization::FrameworkNormalizationExecution,
    ) -> Result<()>;
}

pub trait FrameworkGcmcPort {
    fn run_framework_gcmc(&self, request: &GcmcRequest) -> Result<GcmcResult>;
}

pub trait FrameworkGcmcArtifactSink {
    fn persist_framework_gcmc_run(
        &self,
        execution: &crate::application::framework_gcmc::FrameworkGcmcExecution,
    ) -> Result<()>;
}
