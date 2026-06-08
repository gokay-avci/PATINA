/*!
Building-block records for supramolecular construction.

Phase 1 will introduce typed fragment, placer, and core-atom semantics here.
*/

use nalgebra::Vector3;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use crate::domain::StkDomainError;
use crate::functional_group::{FunctionalGroupBondIntent, FunctionalGroupMatch};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedFragmentRecord {
    pub canonical_smiles: String,
    pub atom_symbols: Vec<String>,
    pub atom_positions: Vec<[f64; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildingBlockRecord {
    label: String,
    functional_group_count: usize,
    local_atom_positions: Vec<[f64; 3]>,
    local_functional_group_vectors: Vec<[f64; 3]>,
    local_functional_group_atom_ids: Vec<usize>,
    local_functional_group_bonder_atom_ids: Vec<Vec<usize>>,
    local_functional_group_deleter_atom_ids: Vec<Vec<usize>>,
    local_functional_group_bond_intents: Vec<FunctionalGroupBondIntent>,
    placer_atom_ids: Vec<usize>,
    core_atom_ids: Vec<usize>,
}

impl Eq for BuildingBlockRecord {}

impl PartialOrd for BuildingBlockRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BuildingBlockRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        self.label
            .cmp(&other.label)
            .then_with(|| {
                self.functional_group_count
                    .cmp(&other.functional_group_count)
            })
            .then_with(|| cmp_vec3_list(&self.local_atom_positions, &other.local_atom_positions))
            .then_with(|| {
                cmp_vec3_list(
                    &self.local_functional_group_vectors,
                    &other.local_functional_group_vectors,
                )
            })
            .then_with(|| {
                self.local_functional_group_atom_ids
                    .cmp(&other.local_functional_group_atom_ids)
            })
            .then_with(|| {
                self.local_functional_group_bonder_atom_ids
                    .cmp(&other.local_functional_group_bonder_atom_ids)
            })
            .then_with(|| {
                self.local_functional_group_deleter_atom_ids
                    .cmp(&other.local_functional_group_deleter_atom_ids)
            })
            .then_with(|| {
                self.local_functional_group_bond_intents
                    .cmp(&other.local_functional_group_bond_intents)
            })
            .then_with(|| self.placer_atom_ids.cmp(&other.placer_atom_ids))
            .then_with(|| self.core_atom_ids.cmp(&other.core_atom_ids))
    }
}

impl Hash for BuildingBlockRecord {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.label.hash(state);
        self.functional_group_count.hash(state);
        hash_vec3_list(&self.local_atom_positions, state);
        hash_vec3_list(&self.local_functional_group_vectors, state);
        self.local_functional_group_atom_ids.hash(state);
        self.local_functional_group_bonder_atom_ids.hash(state);
        self.local_functional_group_deleter_atom_ids.hash(state);
        self.local_functional_group_bond_intents.hash(state);
        self.placer_atom_ids.hash(state);
        self.core_atom_ids.hash(state);
    }
}

impl BuildingBlockRecord {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            functional_group_count: 0,
            local_atom_positions: Vec::new(),
            local_functional_group_vectors: Vec::new(),
            local_functional_group_atom_ids: Vec::new(),
            local_functional_group_bonder_atom_ids: Vec::new(),
            local_functional_group_deleter_atom_ids: Vec::new(),
            local_functional_group_bond_intents: Vec::new(),
            placer_atom_ids: Vec::new(),
            core_atom_ids: Vec::new(),
        }
    }

    pub fn from_embedded_fragment(
        label: impl Into<String>,
        fragment: &EmbeddedFragmentRecord,
        functional_groups: &[FunctionalGroupMatch],
    ) -> Result<Self, StkDomainError> {
        if fragment.atom_positions.is_empty() {
            return Err(StkDomainError::InvalidBuildingBlock {
                reason: "embedded fragment must contain atom positions",
            });
        }
        if fragment.atom_symbols.len() != fragment.atom_positions.len() {
            return Err(StkDomainError::InvalidBuildingBlock {
                reason: "embedded fragment atom symbol and position counts must match",
            });
        }
        if functional_groups.is_empty() {
            return Err(StkDomainError::InvalidBuildingBlock {
                reason: "building-block preparation requires at least one functional group",
            });
        }

        let atom_vectors = fragment
            .atom_positions
            .iter()
            .map(|position| Vector3::new(position[0], position[1], position[2]))
            .collect::<Vec<_>>();
        let centroid = atom_vectors
            .iter()
            .copied()
            .reduce(|left, right| left + right)
            .expect("non-empty atom positions")
            / atom_vectors.len() as f64;

        let local_atom_positions = atom_vectors
            .iter()
            .map(|position| to_array(*position - centroid))
            .collect::<Vec<_>>();

        let mut local_functional_group_vectors = Vec::with_capacity(functional_groups.len());
        let mut local_functional_group_atom_ids = Vec::with_capacity(functional_groups.len());
        let mut local_functional_group_bonder_atom_ids =
            Vec::with_capacity(functional_groups.len());
        let mut local_functional_group_deleter_atom_ids =
            Vec::with_capacity(functional_groups.len());
        let mut local_functional_group_bond_intents = Vec::with_capacity(functional_groups.len());
        let mut placer_atom_ids = Vec::new();
        let mut functional_group_atom_mask = vec![false; atom_vectors.len()];
        for functional_group in functional_groups {
            let bonder_source = if functional_group.bonder_atom_ids.is_empty() {
                &functional_group.atom_ids
            } else {
                &functional_group.bonder_atom_ids
            };
            let placer_source = if functional_group.placer_atom_ids.is_empty() {
                bonder_source
            } else {
                &functional_group.placer_atom_ids
            };

            let Some(&bonder_atom_id) = bonder_source.first() else {
                return Err(StkDomainError::InvalidBuildingBlock {
                    reason: "functional groups must include at least one atom id",
                });
            };
            if bonder_atom_id >= atom_vectors.len() {
                return Err(StkDomainError::InvalidBuildingBlock {
                    reason: "functional-group atom ids must reference valid atoms",
                });
            }

            for &atom_id in &functional_group.atom_ids {
                if atom_id >= atom_vectors.len() {
                    return Err(StkDomainError::InvalidBuildingBlock {
                        reason: "functional-group atom ids must reference valid atoms",
                    });
                }
                functional_group_atom_mask[atom_id] = true;
            }
            placer_atom_ids.extend(placer_source.iter().copied());

            let fg_centroid = placer_source
                .iter()
                .map(|&atom_id| {
                    atom_vectors
                        .get(atom_id)
                        .copied()
                        .ok_or(StkDomainError::InvalidBuildingBlock {
                            reason: "functional-group atom ids must reference valid atoms",
                        })
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .reduce(|left, right| left + right)
                .expect("placer atom ids are non-empty")
                / placer_source.len() as f64;

            local_functional_group_vectors.push(to_array(fg_centroid - centroid));
            local_functional_group_atom_ids.push(bonder_atom_id);
            local_functional_group_bonder_atom_ids.push(bonder_source.to_vec());
            local_functional_group_deleter_atom_ids.push(functional_group.deleter_atom_ids.clone());
            local_functional_group_bond_intents.push(functional_group.bond_intent);
        }

        placer_atom_ids.sort_unstable();
        placer_atom_ids.dedup();
        let core_atom_ids = functional_group_atom_mask
            .iter()
            .enumerate()
            .filter_map(|(atom_id, in_functional_group)| (!in_functional_group).then_some(atom_id))
            .collect::<Vec<_>>();

        Ok(Self::new(label)
            .with_functional_group_count(functional_groups.len())
            .with_local_atom_positions(local_atom_positions)
            .with_local_functional_group_vectors(local_functional_group_vectors)
            .with_local_functional_group_atom_ids(local_functional_group_atom_ids)
            .with_local_functional_group_bonder_atom_ids(local_functional_group_bonder_atom_ids)
            .with_local_functional_group_deleter_atom_ids(local_functional_group_deleter_atom_ids)
            .with_local_functional_group_bond_intents(local_functional_group_bond_intents)
            .with_placer_atom_ids(placer_atom_ids)
            .with_core_atom_ids(core_atom_ids))
    }

    pub fn with_functional_group_count(mut self, functional_group_count: usize) -> Self {
        self.functional_group_count = functional_group_count;
        self
    }

    pub fn with_local_atom_positions(mut self, local_atom_positions: Vec<[f64; 3]>) -> Self {
        self.local_atom_positions = local_atom_positions;
        self
    }

    pub fn with_local_functional_group_vectors(
        mut self,
        local_functional_group_vectors: Vec<[f64; 3]>,
    ) -> Self {
        self.local_functional_group_vectors = local_functional_group_vectors;
        self
    }

    pub fn with_local_functional_group_atom_ids(
        mut self,
        local_functional_group_atom_ids: Vec<usize>,
    ) -> Self {
        self.local_functional_group_bonder_atom_ids = local_functional_group_atom_ids
            .iter()
            .map(|&atom_id| vec![atom_id])
            .collect();
        self.local_functional_group_bond_intents =
            vec![FunctionalGroupBondIntent::Covalent; local_functional_group_atom_ids.len()];
        self.local_functional_group_atom_ids = local_functional_group_atom_ids;
        self
    }

    pub fn with_local_functional_group_bonder_atom_ids(
        mut self,
        local_functional_group_bonder_atom_ids: Vec<Vec<usize>>,
    ) -> Self {
        self.local_functional_group_atom_ids = local_functional_group_bonder_atom_ids
            .iter()
            .filter_map(|atom_ids| atom_ids.first().copied())
            .collect();
        if self.local_functional_group_bond_intents.len()
            != local_functional_group_bonder_atom_ids.len()
        {
            self.local_functional_group_bond_intents = vec![
                FunctionalGroupBondIntent::Covalent;
                local_functional_group_bonder_atom_ids
                    .len()
            ];
        }
        self.local_functional_group_bonder_atom_ids = local_functional_group_bonder_atom_ids;
        self
    }

    pub fn with_local_functional_group_deleter_atom_ids(
        mut self,
        local_functional_group_deleter_atom_ids: Vec<Vec<usize>>,
    ) -> Self {
        self.local_functional_group_deleter_atom_ids = local_functional_group_deleter_atom_ids;
        self
    }

    pub fn with_local_functional_group_bond_intents(
        mut self,
        local_functional_group_bond_intents: Vec<FunctionalGroupBondIntent>,
    ) -> Self {
        self.local_functional_group_bond_intents = local_functional_group_bond_intents;
        self
    }

    pub fn with_placer_atom_ids(mut self, placer_atom_ids: Vec<usize>) -> Self {
        self.placer_atom_ids = placer_atom_ids;
        self
    }

    pub fn with_core_atom_ids(mut self, core_atom_ids: Vec<usize>) -> Self {
        self.core_atom_ids = core_atom_ids;
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn functional_group_count(&self) -> usize {
        self.functional_group_count
    }

    pub fn local_atom_positions(&self) -> &[[f64; 3]] {
        &self.local_atom_positions
    }

    pub fn local_functional_group_vectors(&self) -> &[[f64; 3]] {
        &self.local_functional_group_vectors
    }

    pub fn local_functional_group_atom_ids(&self) -> &[usize] {
        &self.local_functional_group_atom_ids
    }

    pub fn local_functional_group_bonder_atom_ids(&self) -> &[Vec<usize>] {
        &self.local_functional_group_bonder_atom_ids
    }

    pub fn local_functional_group_deleter_atom_ids(&self) -> &[Vec<usize>] {
        &self.local_functional_group_deleter_atom_ids
    }

    pub fn local_functional_group_bond_intents(&self) -> &[FunctionalGroupBondIntent] {
        &self.local_functional_group_bond_intents
    }

    pub fn placer_atom_ids(&self) -> &[usize] {
        &self.placer_atom_ids
    }

    pub fn core_atom_ids(&self) -> &[usize] {
        &self.core_atom_ids
    }
}

fn to_array(value: Vector3<f64>) -> [f64; 3] {
    [value.x, value.y, value.z]
}

fn cmp_vec3_list(left: &[[f64; 3]], right: &[[f64; 3]]) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| {
        left.iter()
            .zip(right.iter())
            .map(|(left, right)| cmp_vec3(*left, *right))
            .find(|ordering| *ordering != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    })
}

fn cmp_vec3(left: [f64; 3], right: [f64; 3]) -> Ordering {
    left[0]
        .total_cmp(&right[0])
        .then_with(|| left[1].total_cmp(&right[1]))
        .then_with(|| left[2].total_cmp(&right[2]))
}

fn hash_vec3_list<H: Hasher>(values: &[[f64; 3]], state: &mut H) {
    values.len().hash(state);
    for value in values {
        value[0].to_bits().hash(state);
        value[1].to_bits().hash(state);
        value[2].to_bits().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use crate::functional_group::{FunctionalGroupBondIntent, FunctionalGroupMatch};

    use super::{BuildingBlockRecord, EmbeddedFragmentRecord};

    #[test]
    fn building_block_can_be_prepared_from_embedded_fragment() {
        let fragment = EmbeddedFragmentRecord {
            canonical_smiles: "NCCN".to_string(),
            atom_symbols: vec![
                "N".to_string(),
                "C".to_string(),
                "C".to_string(),
                "N".to_string(),
            ],
            atom_positions: vec![
                [-2.0, 0.0, 0.0],
                [-1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
            ],
        };
        let functional_groups = vec![
            FunctionalGroupMatch {
                kind: "primary_amino".to_string(),
                bond_intent: FunctionalGroupBondIntent::Covalent,
                atom_ids: vec![0],
                bonder_atom_ids: vec![0],
                deleter_atom_ids: vec![],
                placer_atom_ids: vec![0],
            },
            FunctionalGroupMatch {
                kind: "primary_amino".to_string(),
                bond_intent: FunctionalGroupBondIntent::Covalent,
                atom_ids: vec![3],
                bonder_atom_ids: vec![3],
                deleter_atom_ids: vec![],
                placer_atom_ids: vec![3],
            },
        ];

        let building_block = BuildingBlockRecord::from_embedded_fragment(
            "ethylenediamine",
            &fragment,
            &functional_groups,
        )
        .expect("prepare building block");

        assert_eq!(building_block.functional_group_count(), 2);
        assert_eq!(
            building_block.local_atom_positions(),
            &[
                [-2.0, 0.0, 0.0],
                [-1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [2.0, 0.0, 0.0]
            ]
        );
        assert_eq!(
            building_block.local_functional_group_vectors(),
            &[[-2.0, 0.0, 0.0], [2.0, 0.0, 0.0]]
        );
        assert_eq!(building_block.local_functional_group_atom_ids(), &[0, 3]);
        assert_eq!(building_block.placer_atom_ids(), &[0, 3]);
        assert_eq!(building_block.core_atom_ids(), &[1, 2]);
        assert_eq!(
            building_block.local_functional_group_bond_intents(),
            &[
                FunctionalGroupBondIntent::Covalent,
                FunctionalGroupBondIntent::Covalent,
            ]
        );
    }
}
