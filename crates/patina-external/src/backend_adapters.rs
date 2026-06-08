#![forbid(unsafe_code)]

/*!
File-based backend adapters that hide external evaluators behind readable Rust types and isolated
working directories.
Design basis: prompt Sections 2, 4, 6, and 13 require subprocess-only integration, robust
error reporting, and fixture-driven parser tests before live KLMC3 integration.
Assumption: Phase 1 uses a template injection marker plus a sidecar atom-count expectation
encoded as `# KLMC3-RS EXPECT_ATOMS: <n>` because the upstream `.gin` templates do not
provide a stable machine-readable count near the injection point.
*/

#[path = "backend_common.rs"]
mod common;
#[path = "backend_gin.rs"]
mod gin;
#[path = "backend_gulp.rs"]
mod gulp;
#[path = "backend_janus.rs"]
mod janus;

pub use common::{BackendEvaluator, EvalError, MockBackend};
pub use gin::{GinWriter, INJECTION_MARKER};
pub use gulp::{GotParser, GulpBackend, ScottBackend, ScottSandboxTemplate};
pub use janus::{
    JanusMaceBackend, JanusMaceConfig, JanusMode, JanusOptimizer, PersistentJanusMaceBackend,
};

pub(crate) use common::{
    absolute_workdir, cartesian_coords, cartesian_to_fractional, truncate_stderr,
    write_candidate_xyz, CompletedProcess,
};
pub(crate) use gulp::{parse_got_energy_only_file, patch_gin_for_single_point};

#[cfg(test)]
#[path = "backend_adapters_tests.rs"]
mod tests;
