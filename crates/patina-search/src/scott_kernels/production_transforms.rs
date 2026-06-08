use patina_types::Candidate;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Compact ionic-radius input used by Scott production data-mining transforms.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductionAtomSpec {
    pub species: String,
    pub ionic_radius: f64,
}

/// Compact Scott data-mining transform plan.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductionDataMiningPlan {
    pub replacements: Vec<(String, String)>,
    pub coordinate_scale: f64,
    pub recentre: bool,
}

/// Pure transform error returned by Scott production kernels.
#[derive(Debug, Clone, PartialEq)]
pub enum ProductionTransformError {
    EmptyReplaceAtoms,
    InvalidReplacementPair {
        pair: String,
    },
    TooManyReplacementPairs {
        count: usize,
        raw: String,
    },
    MissingIonicRadius {
        species: String,
    },
    ZeroSourceRadiusSum,
    MasterSpeciesCountMismatch {
        candidate_label: String,
        candidate_atoms: usize,
        master_atoms: usize,
    },
    PeriodicCandidateNotSupported {
        candidate_label: String,
    },
}

impl Display for ProductionTransformError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyReplaceAtoms => {
                write!(f, "`DM_FLAG` is enabled but `DM_REPLACE_ATOMS` is empty")
            }
            Self::InvalidReplacementPair { pair } => {
                write!(f, "failed to parse `DM_REPLACE_ATOMS` pair `{pair}`")
            }
            Self::TooManyReplacementPairs { count, raw } => write!(
                f,
                "native Scott data mining supports at most two replacement pairs; found {count} in `{raw}`"
            ),
            Self::MissingIonicRadius { species } => {
                write!(f, "failed to locate ionic radius for species `{species}`")
            }
            Self::ZeroSourceRadiusSum => write!(
                f,
                "cannot derive data-mining coordinate scale because the source ionic-radius sum is zero"
            ),
            Self::MasterSpeciesCountMismatch {
                candidate_label,
                candidate_atoms,
                master_atoms,
            } => write!(
                f,
                "candidate `{candidate_label}` has {candidate_atoms} atoms but the master template defines {master_atoms} atoms; `L_ENFORCE_MASTER` requires matching counts"
            ),
            Self::PeriodicCandidateNotSupported { candidate_label } => write!(
                f,
                "data-mining production transforms are only supported for non-periodic restart candidates; `{candidate_label}` is periodic"
            ),
        }
    }
}

impl Error for ProductionTransformError {}

pub fn build_production_data_mining_plan(
    replacements_raw: &str,
    recentre: bool,
    atom_specs: &[ProductionAtomSpec],
) -> Result<ProductionDataMiningPlan, ProductionTransformError> {
    let replacements = parse_data_mining_replacements(replacements_raw)?;
    if replacements.is_empty() {
        return Err(ProductionTransformError::EmptyReplaceAtoms);
    }
    if replacements.len() > 2 {
        return Err(ProductionTransformError::TooManyReplacementPairs {
            count: replacements.len(),
            raw: replacements_raw.to_string(),
        });
    }

    let from_radius = replacements.iter().try_fold(0.0, |acc, (from, _)| {
        Ok::<_, ProductionTransformError>(acc + ionic_radius_for_species(atom_specs, from)?)
    })?;
    let to_radius = replacements.iter().try_fold(0.0, |acc, (_, to)| {
        Ok::<_, ProductionTransformError>(acc + ionic_radius_for_species(atom_specs, to)?)
    })?;
    if from_radius.abs() < 1.0e-12 {
        return Err(ProductionTransformError::ZeroSourceRadiusSum);
    }

    Ok(ProductionDataMiningPlan {
        replacements,
        coordinate_scale: to_radius / from_radius,
        recentre,
    })
}

pub fn extract_master_species_from_atom_block(atom_block_lines: &[String]) -> Vec<String> {
    atom_block_lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.eq_ignore_ascii_case("cartesian")
                || trimmed.eq_ignore_ascii_case("fractional")
                || trimmed.starts_with("cartesian region ")
                || trimmed.starts_with("fractional region ")
                || trimmed.eq_ignore_ascii_case("extra")
            {
                return None;
            }

            let parts = trimmed.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 4 {
                return None;
            }
            let coordinate_offset = if parts[1].parse::<f64>().is_ok() {
                1
            } else {
                2
            };
            if parts.len() < coordinate_offset + 3
                || !parts[coordinate_offset..coordinate_offset + 3]
                    .iter()
                    .all(|value| value.parse::<f64>().is_ok())
            {
                return None;
            }
            Some(parts[0].to_string())
        })
        .collect()
}

pub fn enforce_production_master_species(
    candidate: &mut Candidate,
    master_species: &[String],
) -> Result<(), ProductionTransformError> {
    if candidate.species.len() != master_species.len() {
        return Err(ProductionTransformError::MasterSpeciesCountMismatch {
            candidate_label: candidate.label.clone(),
            candidate_atoms: candidate.species.len(),
            master_atoms: master_species.len(),
        });
    }
    candidate.species = master_species.to_vec();
    Ok(())
}

pub fn apply_production_data_mining_to_species(
    species: &mut [String],
    plan: &ProductionDataMiningPlan,
) {
    for atom_species in species {
        for (from, to) in &plan.replacements {
            if atom_species == from {
                *atom_species = to.clone();
            }
        }
    }
}

pub fn apply_production_data_mining(
    candidate: &mut Candidate,
    plan: &ProductionDataMiningPlan,
) -> Result<(), ProductionTransformError> {
    if candidate.lattice.is_some() || candidate.periodic_axes.iter().any(|enabled| *enabled) {
        return Err(ProductionTransformError::PeriodicCandidateNotSupported {
            candidate_label: candidate.label.clone(),
        });
    }
    if candidate.fractional_coords.is_empty() {
        return Ok(());
    }

    apply_production_data_mining_to_species(&mut candidate.species, plan);

    let anchor = candidate.fractional_coords[0];
    for coords in candidate.fractional_coords.iter_mut().skip(1) {
        coords[0] = (coords[0] - anchor[0]) * plan.coordinate_scale + anchor[0];
        coords[1] = (coords[1] - anchor[1]) * plan.coordinate_scale + anchor[1];
        coords[2] = (coords[2] - anchor[2]) * plan.coordinate_scale + anchor[2];
    }

    if plan.recentre {
        recentre_candidate(candidate);
    }

    Ok(())
}

fn parse_data_mining_replacements(
    raw: &str,
) -> Result<Vec<(String, String)>, ProductionTransformError> {
    let cleaned = raw.trim().trim_matches('\'').trim_matches('"');
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }

    cleaned
        .split(';')
        .map(str::trim)
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let Some((from, to)) = pair.split_once('-') else {
                return Err(ProductionTransformError::InvalidReplacementPair {
                    pair: pair.to_string(),
                });
            };
            let from = from.trim();
            let to = to.trim();
            if from.is_empty() || to.is_empty() {
                return Err(ProductionTransformError::InvalidReplacementPair {
                    pair: pair.to_string(),
                });
            }
            Ok((from.to_string(), to.to_string()))
        })
        .collect()
}

fn ionic_radius_for_species(
    atom_specs: &[ProductionAtomSpec],
    species: &str,
) -> Result<f64, ProductionTransformError> {
    atom_specs
        .iter()
        .find(|record| record.species == species)
        .map(|record| record.ionic_radius)
        .ok_or_else(|| ProductionTransformError::MissingIonicRadius {
            species: species.to_string(),
        })
}

fn recentre_candidate(candidate: &mut Candidate) {
    if candidate.fractional_coords.is_empty() {
        return;
    }

    let mut x_min = candidate.fractional_coords[0][0];
    let mut y_min = candidate.fractional_coords[0][1];
    let mut z_min = candidate.fractional_coords[0][2];
    let mut x_max = x_min;
    let mut y_max = y_min;
    let mut z_max = z_min;

    for coords in candidate.fractional_coords.iter().skip(1) {
        x_min = x_min.min(coords[0]);
        y_min = y_min.min(coords[1]);
        z_min = z_min.min(coords[2]);
        x_max = x_max.max(coords[0]);
        y_max = y_max.max(coords[1]);
        z_max = z_max.max(coords[2]);
    }

    let x_mid = 0.5 * (x_min + x_max);
    let y_mid = 0.5 * (y_min + y_max);
    let z_mid = 0.5 * (z_min + z_max);
    for coords in &mut candidate.fractional_coords {
        coords[0] -= x_mid;
        coords[1] -= y_mid;
        coords[2] -= z_mid;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_production_data_mining, apply_production_data_mining_to_species,
        build_production_data_mining_plan, enforce_production_master_species,
        extract_master_species_from_atom_block, ProductionAtomSpec, ProductionDataMiningPlan,
        ProductionTransformError,
    };
    use patina_types::Candidate;

    fn atom_specs() -> Vec<ProductionAtomSpec> {
        vec![
            ProductionAtomSpec {
                species: "Mg".into(),
                ionic_radius: 0.72,
            },
            ProductionAtomSpec {
                species: "O".into(),
                ionic_radius: 1.4,
            },
            ProductionAtomSpec {
                species: "Si".into(),
                ionic_radius: 0.4,
            },
            ProductionAtomSpec {
                species: "F".into(),
                ionic_radius: 1.33,
            },
        ]
    }

    #[test]
    fn extracts_master_species_from_atom_block_lines() {
        let lines = vec![
            "cartesian".to_string(),
            "Mg core 0.0 0.0 0.0".to_string(),
            "O core 1.0 1.0 1.0".to_string(),
            "extra".to_string(),
        ];
        assert_eq!(
            extract_master_species_from_atom_block(&lines),
            vec!["Mg", "O"]
        );
    }

    #[test]
    fn enforce_master_species_requires_matching_count() {
        let mut candidate = Candidate::cluster(
            "seed",
            vec!["Si".into(), "F".into()],
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        );
        enforce_production_master_species(&mut candidate, &["Mg".into(), "O".into()]).unwrap();
        assert_eq!(candidate.species, vec!["Mg", "O"]);
    }

    #[test]
    fn build_data_mining_plan_uses_ionic_radius_ratio() {
        let plan = build_production_data_mining_plan("Mg-Si;O-F", true, &atom_specs()).unwrap();
        assert_eq!(
            plan.replacements,
            vec![
                ("Mg".to_string(), "Si".to_string()),
                ("O".to_string(), "F".to_string())
            ]
        );
        assert!((plan.coordinate_scale - ((0.4 + 1.33) / (0.72 + 1.4))).abs() < 1.0e-12);
        assert!(plan.recentre);
    }

    #[test]
    fn build_data_mining_plan_rejects_empty_replacements() {
        let error = build_production_data_mining_plan("", true, &atom_specs()).expect_err("empty");
        assert_eq!(error, ProductionTransformError::EmptyReplaceAtoms);
    }

    #[test]
    fn apply_data_mining_rewrites_species_and_recentres() {
        let mut candidate = Candidate::cluster(
            "seed",
            vec!["Mg".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
        );
        let plan = ProductionDataMiningPlan {
            replacements: vec![("Mg".into(), "Si".into()), ("O".into(), "F".into())],
            coordinate_scale: 0.5,
            recentre: true,
        };

        apply_production_data_mining(&mut candidate, &plan).unwrap();
        assert_eq!(candidate.species, vec!["Si", "F"]);
        assert_eq!(candidate.fractional_coords[0], [-0.5, 0.0, 0.0]);
        assert_eq!(candidate.fractional_coords[1], [0.5, 0.0, 0.0]);
    }

    #[test]
    fn apply_data_mining_to_species_updates_in_place() {
        let plan = ProductionDataMiningPlan {
            replacements: vec![("Mg".into(), "Si".into()), ("O".into(), "F".into())],
            coordinate_scale: 1.0,
            recentre: false,
        };
        let mut species = vec!["Mg".into(), "O".into(), "Mg".into()];

        apply_production_data_mining_to_species(&mut species, &plan);
        assert_eq!(species, vec!["Si", "F", "Si"]);
    }
}
