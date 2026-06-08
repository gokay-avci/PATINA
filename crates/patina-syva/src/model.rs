use crate::error::SyvaError;
use crate::periodic_table::{
    atomic_number_for_symbol, canonicalize_species_symbol, symbol_for_atomic_number,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterAtom {
    pub species: String,
    pub cartesian: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterGeometry {
    pub label: String,
    pub atoms: Vec<ClusterAtom>,
}

impl ClusterGeometry {
    pub fn validate(&self) -> Result<(), SyvaError> {
        if self.atoms.is_empty() {
            return Err(SyvaError::EmptyCluster);
        }

        for (index, atom) in self.atoms.iter().enumerate() {
            if atom.species.trim().is_empty() {
                return Err(SyvaError::EmptySpecies { index });
            }
            if atom.cartesian.iter().any(|value| !value.is_finite()) {
                return Err(SyvaError::NonFiniteCoordinate {
                    index,
                    coordinate: atom.cartesian,
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyvaInputAtom {
    pub atomic_number: u8,
    pub species: String,
    pub cartesian: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyvaSubsetSpec {
    pub atom_count: usize,
    pub atom_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyvaInputGeometry {
    pub title: String,
    pub atoms: Vec<SyvaInputAtom>,
    pub subset: Option<SyvaSubsetSpec>,
}

impl SyvaInputGeometry {
    pub fn from_cluster_geometry(cluster: &ClusterGeometry) -> Result<Self, SyvaError> {
        cluster.validate()?;
        let atoms = cluster
            .atoms
            .iter()
            .enumerate()
            .map(|(index, atom)| {
                let species = canonicalize_species_symbol(&atom.species).ok_or_else(|| {
                    SyvaError::UnsupportedSpecies {
                        index,
                        species: atom.species.trim().to_string(),
                    }
                })?;
                let atomic_number = atomic_number_for_species(&species).ok_or_else(|| {
                    SyvaError::UnsupportedSpecies {
                        index,
                        species: species.clone(),
                    }
                })?;
                Ok(SyvaInputAtom {
                    atomic_number,
                    species,
                    cartesian: atom.cartesian,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            title: cluster.label.clone(),
            atoms,
            subset: None,
        })
    }

    pub fn render_input_text(&self) -> String {
        let mut lines = Vec::with_capacity(self.atoms.len() + 4);
        lines.push(self.title.clone());
        lines.push(self.atoms.len().to_string());
        lines.extend(self.atoms.iter().map(|atom| {
            format!(
                "{} {:.10} {:.10} {:.10}",
                atom.atomic_number, atom.cartesian[0], atom.cartesian[1], atom.cartesian[2]
            )
        }));
        if let Some(subset) = &self.subset {
            lines.push(subset.atom_count.to_string());
            lines.push(
                subset
                    .atom_indices
                    .iter()
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointGroupLabel(String);

impl PointGroupLabel {
    pub fn new(label: impl Into<String>) -> Result<Self, SyvaError> {
        let normalized = label.into().trim().to_string();
        if normalized.is_empty() {
            return Err(SyvaError::EmptyPointGroup);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionSettings {
    pub tolerance: f64,
}

impl DetectionSettings {
    pub fn validate(&self) -> Result<(), SyvaError> {
        if !self.tolerance.is_finite() || self.tolerance <= 0.0 {
            return Err(SyvaError::InvalidTolerance(self.tolerance));
        }
        Ok(())
    }
}

impl Default for DetectionSettings {
    fn default() -> Self {
        Self { tolerance: 0.01 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionResult {
    pub point_group: PointGroupLabel,
    pub operation_count: usize,
    pub max_deviation: Option<f64>,
}

pub(crate) fn atomic_number_for_species(species: &str) -> Option<u8> {
    atomic_number_for_symbol(species)
}

pub fn species_for_atomic_number(atomic_number: u8) -> Option<&'static str> {
    symbol_for_atomic_number(atomic_number)
}
