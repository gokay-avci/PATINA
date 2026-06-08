/*!
Finite polymer topology definitions.

The current campaign treats polymer work as finite 0D/cluster-like construction first,
not infinite periodic chain management. The implementation here focuses on oligomer
state generation which can later feed optimizers or periodic adapters.
*/

use std::collections::BTreeMap;

use crate::building_block::BuildingBlockRecord;
use crate::domain::StkDomainError;
use crate::topology::{
    edge::TopologyEdge,
    libraries::zero_d::{ZeroDTopologyBuilder, ZeroDTopologyPlan},
    vertex::{TopologyVertex, VertexKind},
};

pub fn finite_polymer_plan(
    backbone: Vec<BuildingBlockRecord>,
    head_cap: Option<BuildingBlockRecord>,
    tail_cap: Option<BuildingBlockRecord>,
    spacing: f64,
) -> Result<ZeroDTopologyPlan, StkDomainError> {
    let head_cap_present = head_cap.is_some();
    let tail_cap_present = tail_cap.is_some();

    if backbone.is_empty() {
        return Err(StkDomainError::InvalidPlacement {
            reason: "finite polymer construction requires at least one backbone vertex",
        });
    }
    if spacing <= 0.0 {
        return Err(StkDomainError::InvalidPlacement {
            reason: "finite polymer construction requires a positive spacing",
        });
    }
    if backbone
        .iter()
        .any(|building_block| building_block.functional_group_count() != 2)
    {
        return Err(StkDomainError::InvalidPlacement {
            reason: "finite polymer backbone requires two-functional-group building blocks",
        });
    }
    if head_cap
        .as_ref()
        .is_some_and(|building_block| building_block.functional_group_count() != 1)
        || tail_cap
            .as_ref()
            .is_some_and(|building_block| building_block.functional_group_count() != 1)
    {
        return Err(StkDomainError::InvalidPlacement {
            reason: "finite polymer caps require one functional group",
        });
    }

    let mut builder = ZeroDTopologyBuilder::new();
    let mut placements = BTreeMap::<BuildingBlockRecord, Vec<usize>>::new();
    let mut edge_id = 0usize;
    let mut next_vertex_id = 0usize;

    if let Some(head_cap) = head_cap {
        builder = builder.add_vertex(
            TopologyVertex::new(next_vertex_id, [-spacing, 0.0, 0.0])
                .with_kind(VertexKind::Unaligning),
        );
        placements.entry(head_cap).or_default().push(next_vertex_id);
        next_vertex_id += 1;
    }

    let backbone_start = next_vertex_id;
    let backbone_len = backbone.len();
    for (offset, building_block) in backbone.into_iter().enumerate() {
        let vertex_id = backbone_start + offset;
        let has_left_edge = offset > 0 || head_cap_present;
        let has_right_edge = offset + 1 < backbone_len || tail_cap_present;
        let kind = if has_left_edge && has_right_edge {
            VertexKind::Linear
        } else {
            VertexKind::Unaligning
        };
        builder = builder.add_vertex(
            TopologyVertex::new(vertex_id, [offset as f64 * spacing, 0.0, 0.0])
                .with_kind(kind)
                .with_aligner_edge(0),
        );
        placements
            .entry(building_block)
            .or_default()
            .push(vertex_id);
        next_vertex_id += 1;
    }

    if let Some(tail_cap) = tail_cap {
        builder = builder.add_vertex(
            TopologyVertex::new(next_vertex_id, [backbone_len as f64 * spacing, 0.0, 0.0])
                .with_kind(VertexKind::Unaligning),
        );
        placements.entry(tail_cap).or_default().push(next_vertex_id);
        next_vertex_id += 1;
    }

    let first_backbone_vertex = backbone_start;
    let last_backbone_vertex = backbone_start + backbone_len - 1;

    if head_cap_present {
        builder = builder.add_edge(
            TopologyEdge::new(edge_id, [0, first_backbone_vertex]).with_parent_id(edge_id),
        );
        edge_id += 1;
    }

    for vertex_id in first_backbone_vertex..last_backbone_vertex {
        builder = builder.add_edge(
            TopologyEdge::new(edge_id, [vertex_id, vertex_id + 1]).with_parent_id(edge_id),
        );
        edge_id += 1;
    }

    if tail_cap_present {
        builder = builder.add_edge(
            TopologyEdge::new(edge_id, [last_backbone_vertex, next_vertex_id - 1])
                .with_parent_id(edge_id),
        );
    }

    for (building_block, vertex_ids) in placements {
        builder = builder.assign_building_block(building_block, vertex_ids);
    }

    let mut stage = Vec::new();
    if head_cap_present {
        stage.push(0);
    }
    stage.extend(first_backbone_vertex..=last_backbone_vertex);
    if tail_cap_present {
        stage.push(next_vertex_id - 1);
    }

    Ok(builder.add_stage(stage).build())
}

pub fn capped_ab_polymer_plan(
    num_repeating_units: usize,
) -> Result<ZeroDTopologyPlan, StkDomainError> {
    if num_repeating_units == 0 {
        return Err(StkDomainError::InvalidPlacement {
            reason: "finite polymer repeating-unit count must be greater than zero",
        });
    }

    let mut backbone = Vec::with_capacity(num_repeating_units * 2);
    for _ in 0..num_repeating_units {
        backbone.push(backbone_linker("polymer_a"));
        backbone.push(backbone_linker("polymer_b"));
    }

    finite_polymer_plan(backbone, Some(cap("head_cap")), Some(cap("tail_cap")), 1.5)
}

fn backbone_linker(label: &str) -> BuildingBlockRecord {
    BuildingBlockRecord::new(label)
        .with_functional_group_count(2)
        .with_local_atom_positions(vec![[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
        .with_local_functional_group_vectors(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
        .with_local_functional_group_atom_ids(vec![0, 2])
}

fn cap(label: &str) -> BuildingBlockRecord {
    BuildingBlockRecord::new(label)
        .with_functional_group_count(1)
        .with_local_atom_positions(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
        .with_local_functional_group_atom_ids(vec![0])
}

#[cfg(test)]
mod tests {
    use super::{capped_ab_polymer_plan, finite_polymer_plan};
    use crate::building_block::BuildingBlockRecord;
    use crate::domain::StkDomainError;

    #[test]
    fn capped_polymer_constructs() {
        let result = capped_ab_polymer_plan(2)
            .expect("valid capped polymer")
            .construct_with_default_driver()
            .expect("construct capped polymer");
        assert_eq!(result.molecule_state().num_placements(), 6);
        assert_eq!(result.molecule_state().bonds().len(), 5);
    }

    #[test]
    fn polymer_rejects_non_linear_backbone_units() {
        let result = finite_polymer_plan(
            vec![BuildingBlockRecord::new("bad").with_functional_group_count(1)],
            None,
            None,
            1.0,
        );

        assert!(matches!(
            result,
            Err(StkDomainError::InvalidPlacement {
                reason: "finite polymer backbone requires two-functional-group building blocks",
            })
        ));
    }

    #[test]
    fn polymer_rejects_invalid_caps() {
        let result = finite_polymer_plan(
            vec![BuildingBlockRecord::new("good").with_functional_group_count(2)],
            Some(BuildingBlockRecord::new("bad_cap").with_functional_group_count(2)),
            None,
            1.0,
        );

        assert!(matches!(
            result,
            Err(StkDomainError::InvalidPlacement {
                reason: "finite polymer caps require one functional group",
            })
        ));
    }
}
