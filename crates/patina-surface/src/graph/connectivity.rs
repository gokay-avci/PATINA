// Source-to-target mapping for this module:
// - `to_integrate_project/crystal_surface_generator/src/core/connectivity.rs`
//
// This first port focuses on slab fragment/component extraction over the bond
// graph. Full molecule coordinate reconstruction remains a later checkpoint once
// the broader surface graph layer is in regular use.

use crate::domain::{SurfaceInterfaceError, SurfaceSlab};
use crate::graph::bonding::{build_bond_graph, BondGraph, BondingConfig};

pub(crate) fn connected_components(
    slab: &SurfaceSlab,
    config: &BondingConfig,
) -> Result<Vec<Vec<usize>>, SurfaceInterfaceError> {
    let graph = build_bond_graph(slab, config)?;
    Ok(graph.connected_components())
}

pub(crate) fn component_count(
    slab: &SurfaceSlab,
    config: &BondingConfig,
) -> Result<usize, SurfaceInterfaceError> {
    Ok(connected_components(slab, config)?.len())
}

pub(crate) fn largest_component_size(graph: &BondGraph) -> usize {
    graph
        .connected_components()
        .into_iter()
        .map(|component| component.len())
        .max()
        .unwrap_or(0)
}
