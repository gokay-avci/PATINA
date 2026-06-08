// Source-to-target mapping for this module:
// - `to_integrate_project/crystal_surface_generator/src/synthesis/reduction.rs`
//
// This module is intentionally a scaffold. Upstream does not yet provide a true
// in-plane primitive finder; it only preserves API shape while delegating to dedup.

use crate::domain::{
    DedupConfig, DedupReport, SurfaceGenerationConfig, SurfaceInterfaceError, SurfaceSlab,
};
use crate::reduction::dedup::dedup_atoms_inplace;

pub(crate) fn apply_slab_reduction(
    slab: &mut SurfaceSlab,
    config: &SurfaceGenerationConfig,
) -> Result<Option<DedupReport>, SurfaceInterfaceError> {
    if config.slab_reduction.reduce_slab_inplane {
        return Ok(Some(reduce_slab_inplane_scaffold(
            slab,
            &config.slab_reduction.dedup,
        )?));
    }
    if config.slab_reduction.dedup_slab {
        return Ok(Some(dedup_atoms_inplace(
            slab,
            &config.slab_reduction.dedup,
        )?));
    }
    Ok(None)
}

pub(crate) fn reduce_slab_inplane_scaffold(
    slab: &mut SurfaceSlab,
    dedup: &DedupConfig,
) -> Result<DedupReport, SurfaceInterfaceError> {
    dedup_atoms_inplace(slab, dedup)
}
