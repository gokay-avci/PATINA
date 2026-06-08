/*!
Chemistry-facing helpers for common organic and inorganic construction roles.

These helpers are deliberately small and explicit. They provide reusable defaults
for RDKit functional-group detection and synthetic coordination centers without
locking the construction kernel into one reaction-factory model.
*/

use crate::building_block::BuildingBlockRecord;
use crate::domain::StkDomainError;
use crate::functional_group::{FunctionalGroupBondIntent, FunctionalGroupPattern};
use crate::ports::chemistry::ChemistryToolkitPort;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationGeometry {
    Linear,
    TrigonalPlanar,
    SquarePlanar,
    Tetrahedral,
    Octahedral,
}

impl CoordinationGeometry {
    pub fn site_vectors(self) -> Vec<[f64; 3]> {
        match self {
            Self::Linear => vec![[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]],
            Self::TrigonalPlanar => vec![
                [1.0, 0.0, 0.0],
                [-0.5, 0.866_025_403_8, 0.0],
                [-0.5, -0.866_025_403_8, 0.0],
            ],
            Self::SquarePlanar => vec![
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [-1.0, 0.0, 0.0],
                [0.0, -1.0, 0.0],
            ],
            Self::Tetrahedral => vec![
                [1.0, 1.0, 1.0],
                [1.0, -1.0, -1.0],
                [-1.0, 1.0, -1.0],
                [-1.0, -1.0, 1.0],
            ],
            Self::Octahedral => vec![
                [1.0, 0.0, 0.0],
                [-1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, -1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, -1.0],
            ],
        }
    }
}

pub fn amine_coordination_donor_pattern(kind: impl Into<String>) -> FunctionalGroupPattern {
    FunctionalGroupPattern::new(kind, "[N]")
        .with_bond_intent(FunctionalGroupBondIntent::Coordination)
        .with_bonder_indices(vec![0])
        .with_placer_indices(vec![0])
}

pub fn aromatic_nitrogen_coordination_donor_pattern(
    kind: impl Into<String>,
) -> FunctionalGroupPattern {
    FunctionalGroupPattern::new(kind, "[n]")
        .with_bond_intent(FunctionalGroupBondIntent::Coordination)
        .with_bonder_indices(vec![0])
        .with_placer_indices(vec![0])
}

pub fn bromo_covalent_leaving_group_pattern(kind: impl Into<String>) -> FunctionalGroupPattern {
    FunctionalGroupPattern::new(kind, "[Br][#6]")
        .with_bond_intent(FunctionalGroupBondIntent::Covalent)
        .with_bonder_indices(vec![1])
        .with_deleter_indices(vec![0])
        .with_placer_indices(vec![1])
}

pub fn iodo_covalent_leaving_group_pattern(kind: impl Into<String>) -> FunctionalGroupPattern {
    FunctionalGroupPattern::new(kind, "[I][#6]")
        .with_bond_intent(FunctionalGroupBondIntent::Covalent)
        .with_bonder_indices(vec![1])
        .with_deleter_indices(vec![0])
        .with_placer_indices(vec![1])
}

pub fn coordination_center(
    label: impl Into<String>,
    geometry: CoordinationGeometry,
) -> BuildingBlockRecord {
    coordination_center_from_site_vectors(label, geometry.site_vectors())
}

pub fn coordination_center_from_site_vectors(
    label: impl Into<String>,
    site_vectors: Vec<[f64; 3]>,
) -> BuildingBlockRecord {
    let site_count = site_vectors.len();
    let mut atom_positions = Vec::with_capacity(site_count + 1);
    atom_positions.push([0.0, 0.0, 0.0]);
    atom_positions.extend(site_vectors.iter().copied());

    BuildingBlockRecord::new(label)
        .with_functional_group_count(site_count)
        .with_local_atom_positions(atom_positions)
        .with_local_functional_group_vectors(site_vectors)
        .with_local_functional_group_bonder_atom_ids(vec![vec![0]; site_count])
        .with_local_functional_group_deleter_atom_ids(vec![Vec::new(); site_count])
        .with_local_functional_group_bond_intents(vec![
            FunctionalGroupBondIntent::Coordination;
            site_count
        ])
        .with_placer_atom_ids((1..=site_count).collect())
        .with_core_atom_ids(vec![0])
}

pub fn prepare_coordination_donor_block(
    toolkit: &impl ChemistryToolkitPort,
    label: impl Into<String>,
    smiles: &str,
    seed: u64,
    patterns: &[FunctionalGroupPattern],
) -> Result<BuildingBlockRecord, StkDomainError> {
    let canonical_smiles = toolkit.canonicalize_smiles(smiles)?;
    let matches = toolkit.detect_functional_groups(&canonical_smiles, patterns)?;
    let fragment = toolkit.embed_conformer(&canonical_smiles, seed)?;
    BuildingBlockRecord::from_embedded_fragment(label, &fragment, &matches)
}

#[cfg(test)]
mod tests {
    use super::{
        amine_coordination_donor_pattern, bromo_covalent_leaving_group_pattern,
        coordination_center, CoordinationGeometry,
    };
    use crate::functional_group::FunctionalGroupBondIntent;

    #[test]
    fn donor_and_leaving_group_patterns_carry_roles() {
        let donor = amine_coordination_donor_pattern("amine_donor");
        assert_eq!(donor.smarts, "[N]");
        assert_eq!(donor.bond_intent, FunctionalGroupBondIntent::Coordination);
        assert_eq!(donor.bonder_indices, vec![0]);
        assert_eq!(donor.placer_indices, vec![0]);

        let bromo = bromo_covalent_leaving_group_pattern("bromo");
        assert_eq!(bromo.smarts, "[Br][#6]");
        assert_eq!(bromo.bond_intent, FunctionalGroupBondIntent::Covalent);
        assert_eq!(bromo.bonder_indices, vec![1]);
        assert_eq!(bromo.deleter_indices, vec![0]);
    }

    #[test]
    fn square_planar_coordination_center_bonds_through_metal_atom() {
        let center = coordination_center("Pd", CoordinationGeometry::SquarePlanar);

        assert_eq!(center.functional_group_count(), 4);
        assert_eq!(center.local_atom_positions().len(), 5);
        assert_eq!(
            center.local_functional_group_bonder_atom_ids(),
            &[vec![0], vec![0], vec![0], vec![0]]
        );
        assert_eq!(
            center.local_functional_group_bond_intents(),
            &[FunctionalGroupBondIntent::Coordination; 4]
        );
        assert_eq!(center.placer_atom_ids(), &[1, 2, 3, 4]);
        assert_eq!(center.core_atom_ids(), &[0]);
    }
}
