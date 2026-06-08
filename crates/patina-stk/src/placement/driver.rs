/*!
Default placement driver backed by the crate's built-in placement kernels.
*/

use crate::building_block::BuildingBlockRecord;
use crate::construction::{engine::PlacementDriver, placement::PlacementResult};
use crate::domain::StkDomainError;
use crate::topology::{
    edge::TopologyEdge,
    vertex::{TopologyVertex, VertexKind},
};

use super::{linear::place_linear_building_block, nonlinear::place_nonlinear_building_block};

pub struct DefaultPlacementDriver;

impl PlacementDriver for DefaultPlacementDriver {
    fn place_building_block(
        &self,
        vertex: &TopologyVertex,
        edges: &[TopologyEdge],
        building_block: &BuildingBlockRecord,
    ) -> Result<PlacementResult, StkDomainError> {
        match vertex.kind() {
            VertexKind::Unaligning => {
                let placed = building_block
                    .local_atom_positions()
                    .iter()
                    .map(|point| {
                        [
                            point[0] + vertex.position()[0],
                            point[1] + vertex.position()[1],
                            point[2] + vertex.position()[2],
                        ]
                    })
                    .collect::<Vec<_>>();
                let fg_map = edges
                    .iter()
                    .enumerate()
                    .map(|(fg_id, edge)| (fg_id, edge.id()))
                    .collect();
                let fg_atom_ids = building_block
                    .local_functional_group_atom_ids()
                    .iter()
                    .enumerate()
                    .map(|(fg_id, &atom_id)| (fg_id, atom_id))
                    .collect();
                let fg_bonder_ids = building_block
                    .local_functional_group_bonder_atom_ids()
                    .iter()
                    .enumerate()
                    .map(|(fg_id, atom_ids)| (fg_id, atom_ids.clone()))
                    .collect();
                let fg_deleter_ids = building_block
                    .local_functional_group_deleter_atom_ids()
                    .iter()
                    .enumerate()
                    .map(|(fg_id, atom_ids)| (fg_id, atom_ids.clone()))
                    .collect();
                let fg_bond_intents = building_block
                    .local_functional_group_bond_intents()
                    .iter()
                    .enumerate()
                    .map(|(fg_id, &bond_intent)| (fg_id, bond_intent))
                    .collect();
                Ok(PlacementResult::new(placed, fg_map, fg_atom_ids)
                    .with_functional_group_metadata(fg_bonder_ids, fg_deleter_ids, fg_bond_intents))
            }
            VertexKind::Linear => place_linear_building_block(vertex, edges, building_block),
            VertexKind::NonLinear => place_nonlinear_building_block(vertex, edges, building_block),
        }
    }
}
