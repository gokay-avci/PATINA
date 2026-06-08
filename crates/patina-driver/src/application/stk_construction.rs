use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use patina_dreadnaut::build_dreadnaut_graph_text_from_parts;
use patina_stk::analysis::{
    describe_construction_geometry, describe_construction_topology, ConstructionGeometryDescriptor,
    ConstructionTopologyDescriptor,
};
use patina_stk::bonding::definition::{BondDefinitionRule, ConstructedBond};
use patina_stk::construction::result::ConstructionResult;
use patina_stk::topology::libraries::{
    cage::{m2l4_lantern_plan, trigonal_cage_plan},
    host_guest::single_guest_complex_plan,
    macrocycle::{alternating_macrocycle_plan, triangle_macrocycle_plan},
    polymer::capped_ab_polymer_plan,
    rotaxane::single_ring_rotaxane_plan,
    zero_d::{
        square_planar_zero_d_plan, three_site_linear_bridge_plan, trigonal_zero_d_plan,
        ZeroDTopologyPlan,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StkZeroDTopologyPreset {
    LinearBridge,
    Trigonal,
    SquarePlanar,
    TrigonalCage,
    M2l4Lantern,
    TriangleMacrocycle,
    AlternatingMacrocycle,
    CappedPolymer,
    HostGuest,
    Rotaxane,
}

impl StkZeroDTopologyPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::LinearBridge => "linear_bridge",
            Self::Trigonal => "trigonal",
            Self::SquarePlanar => "square_planar",
            Self::TrigonalCage => "trigonal_cage",
            Self::M2l4Lantern => "m2l4_lantern",
            Self::TriangleMacrocycle => "triangle_macrocycle",
            Self::AlternatingMacrocycle => "alternating_macrocycle",
            Self::CappedPolymer => "capped_polymer",
            Self::HostGuest => "host_guest",
            Self::Rotaxane => "rotaxane",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StkConstructionRequest {
    pub topology: StkZeroDTopologyPreset,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StkPlacementSummary {
    pub vertex_id: usize,
    pub building_block_label: String,
    pub atom_start: usize,
    pub atom_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StkConstructionSummary {
    pub topology: String,
    pub atom_count: usize,
    pub placement_count: usize,
    pub bond_count: usize,
    pub deleted_atom_count: usize,
    pub bond_counts_by_rule: BTreeMap<String, usize>,
    pub placements: Vec<StkPlacementSummary>,
    pub xyz_path: PathBuf,
    pub summary_path: PathBuf,
    pub result_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StkBondSummary {
    pub atom_ids: (usize, usize),
    pub edge_id: usize,
    pub rule: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StkConstructionArtifact {
    pub summary: StkConstructionSummary,
    pub topology: ConstructionTopologyDescriptor,
    pub geometry: ConstructionGeometryDescriptor,
    pub dreadnaut_graph_text: String,
    pub position_matrix: Vec<[f64; 3]>,
    pub bonds: Vec<StkBondSummary>,
    pub deleted_atom_ids: Vec<usize>,
}

pub fn construct_zero_d_topology(
    request: &StkConstructionRequest,
) -> Result<StkConstructionSummary> {
    fs::create_dir_all(&request.output_dir).with_context(|| {
        format!(
            "failed to create STK construction output directory `{}`",
            request.output_dir.display()
        )
    })?;

    let result = preset_plan(request.topology)
        .construct_with_default_driver()
        .with_context(|| {
            format!(
                "failed to construct STK topology `{}`",
                request.topology.label()
            )
        })?;
    let stem = format!("stk_{}", request.topology.label());
    let xyz_path = request.output_dir.join(format!("{stem}.xyz"));
    let summary_path = request.output_dir.join(format!("{stem}_summary.json"));
    let result_path = request.output_dir.join(format!("{stem}_result.json"));

    write_xyz(&result, request.topology, &xyz_path)?;
    let summary = summarize_result(
        &result,
        request.topology,
        &xyz_path,
        &summary_path,
        &result_path,
    );
    write_json(&summary_path, &summary)?;
    write_json(&result_path, &export_artifact(&result, summary.clone()))?;

    Ok(summary)
}

fn preset_plan(topology: StkZeroDTopologyPreset) -> ZeroDTopologyPlan {
    match topology {
        StkZeroDTopologyPreset::LinearBridge => three_site_linear_bridge_plan(),
        StkZeroDTopologyPreset::Trigonal => trigonal_zero_d_plan(),
        StkZeroDTopologyPreset::SquarePlanar => square_planar_zero_d_plan(),
        StkZeroDTopologyPreset::TrigonalCage => trigonal_cage_plan(),
        StkZeroDTopologyPreset::M2l4Lantern => m2l4_lantern_plan(),
        StkZeroDTopologyPreset::TriangleMacrocycle => triangle_macrocycle_plan(),
        StkZeroDTopologyPreset::AlternatingMacrocycle => {
            alternating_macrocycle_plan(3).expect("fixed alternating macrocycle preset is valid")
        }
        StkZeroDTopologyPreset::CappedPolymer => {
            capped_ab_polymer_plan(2).expect("fixed capped polymer preset is valid")
        }
        StkZeroDTopologyPreset::HostGuest => single_guest_complex_plan(),
        StkZeroDTopologyPreset::Rotaxane => single_ring_rotaxane_plan(),
    }
}

fn export_artifact(
    result: &ConstructionResult,
    summary: StkConstructionSummary,
) -> StkConstructionArtifact {
    let topology = describe_construction_topology(result);
    let dreadnaut_graph_text = build_dreadnaut_graph_text_from_parts(
        topology.automorphism_view.vertex_order.len(),
        &topology.automorphism_view.edges,
        &topology.automorphism_view.color_partitions,
    );
    StkConstructionArtifact {
        summary,
        topology,
        geometry: describe_construction_geometry(result),
        dreadnaut_graph_text,
        position_matrix: result.molecule_state().position_matrix().to_vec(),
        bonds: result.bonds().iter().map(summarize_bond).collect(),
        deleted_atom_ids: result.deleted_atom_ids().to_vec(),
    }
}

fn summarize_result(
    result: &ConstructionResult,
    topology: StkZeroDTopologyPreset,
    xyz_path: &Path,
    summary_path: &Path,
    result_path: &Path,
) -> StkConstructionSummary {
    let bond_counts_by_rule = result
        .bond_counts_by_rule()
        .into_iter()
        .map(|(rule, count)| (bond_rule_label(rule).to_string(), count))
        .collect::<BTreeMap<_, _>>();
    let placements = result
        .molecule_state()
        .placement_instances()
        .iter()
        .map(|placement| StkPlacementSummary {
            vertex_id: placement.vertex_id(),
            building_block_label: placement.building_block().label().to_string(),
            atom_start: placement.atom_start(),
            atom_count: placement.atom_count(),
        })
        .collect();

    StkConstructionSummary {
        topology: topology.label().into(),
        atom_count: result.molecule_state().position_matrix().len(),
        placement_count: result.molecule_state().num_placements(),
        bond_count: result.bonds().len(),
        deleted_atom_count: result.deleted_atom_ids().len(),
        bond_counts_by_rule,
        placements,
        xyz_path: xyz_path.into(),
        summary_path: summary_path.into(),
        result_path: result_path.into(),
    }
}

fn summarize_bond(bond: &ConstructedBond) -> StkBondSummary {
    StkBondSummary {
        atom_ids: bond.atom_ids,
        edge_id: bond.edge_id,
        rule: bond_rule_label(bond.rule).to_string(),
    }
}

fn write_xyz(
    result: &ConstructionResult,
    topology: StkZeroDTopologyPreset,
    path: &Path,
) -> Result<()> {
    let mut file = fs::File::create(path)
        .with_context(|| format!("failed to create STK XYZ artifact `{}`", path.display()))?;
    writeln!(file, "{}", result.molecule_state().position_matrix().len())?;
    writeln!(
        file,
        "patina-stk topology={} placements={} bonds={}",
        topology.label(),
        result.molecule_state().num_placements(),
        result.bonds().len()
    )?;
    for position in result.molecule_state().position_matrix() {
        writeln!(
            file,
            "X {:.10} {:.10} {:.10}",
            position[0], position[1], position[2]
        )?;
    }
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let file = fs::File::create(path)
        .with_context(|| format!("failed to create JSON artifact `{}`", path.display()))?;
    serde_json::to_writer_pretty(file, value)
        .with_context(|| format!("failed to write JSON artifact `{}`", path.display()))
}

fn bond_rule_label(rule: BondDefinitionRule) -> &'static str {
    match rule {
        BondDefinitionRule::CovalentSingleBonder => "covalent_single_bonder",
        BondDefinitionRule::CovalentMultiBonder => "covalent_multi_bonder",
        BondDefinitionRule::DativeSharedEdge => "dative_shared_edge",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        construct_zero_d_topology, StkConstructionArtifact, StkConstructionRequest,
        StkConstructionSummary, StkZeroDTopologyPreset,
    };
    use tempfile::tempdir;

    #[test]
    fn constructs_square_planar_and_writes_stable_artifacts() {
        let temp = tempdir().expect("tempdir");
        let summary = construct_zero_d_topology(&StkConstructionRequest {
            topology: StkZeroDTopologyPreset::SquarePlanar,
            output_dir: temp.path().join("stk"),
        })
        .expect("construct");

        assert_eq!(summary.topology, "square_planar");
        assert_eq!(summary.placement_count, 5);
        assert_eq!(summary.bond_count, 4);
        assert!(summary.xyz_path.exists());
        assert!(summary.summary_path.exists());
        assert!(summary.result_path.exists());

        let summary_json: StkConstructionSummary =
            serde_json::from_slice(&fs::read(&summary.summary_path).expect("summary json"))
                .expect("summary artifact");
        assert_eq!(summary_json.bond_counts_by_rule["dative_shared_edge"], 4);

        let result_json: StkConstructionArtifact =
            serde_json::from_slice(&fs::read(&summary.result_path).expect("result json"))
                .expect("result artifact");
        assert_eq!(result_json.summary, summary_json);
        assert_eq!(result_json.position_matrix.len(), summary.atom_count);
        assert_eq!(result_json.bonds.len(), summary.bond_count);
        assert_eq!(result_json.topology.automorphism_view.edges.len(), 4);
        assert_eq!(result_json.geometry.bonds.len(), 4);
        assert!(result_json.dreadnaut_graph_text.contains("n=5 g"));
        assert!(result_json.dreadnaut_graph_text.contains("\nf=["));
        assert!(result_json
            .bonds
            .iter()
            .all(|bond| bond.rule == "dative_shared_edge"));
    }
}
